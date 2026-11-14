//! Token verification against a locally generated ES256 key, the same shape
//! the accounts service publishes.

use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use p256::SecretKey;
use p256::pkcs8::EncodePrivateKey;
use serde_json::{Value, json};
use toddstack_accounts::{Accounts, Config, Error, ISSUER};

const KID: &str = "test-key";

struct Signer {
    encoding: EncodingKey,
    jwks: JwkSet,
}

fn b64(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn signer() -> Signer {
    let secret: SecretKey = SecretKey::from_slice(&[7_u8; 32]).unwrap();
    let der = secret.to_pkcs8_der().unwrap();
    let point: Box<[u8]> = secret.public_key().to_sec1_bytes();
    let jwks: JwkSet = serde_json::from_value(json!({ "keys": [{
        "kty": "EC", "crv": "P-256", "kid": KID, "use": "sig", "alg": "ES256",
        "x": b64(&point[1..33]), "y": b64(&point[33..65])
    }]}))
    .unwrap();
    Signer {
        encoding: EncodingKey::from_ec_der(der.as_bytes()),
        jwks,
    }
}

fn now() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap()
}

fn sign(s: &Signer, claims: &Value, typ: Option<&str>) -> String {
    let mut header: Header = Header::new(Algorithm::ES256);
    header.kid = Some(String::from(KID));
    header.typ = typ.map(str::to_owned);
    encode(&header, claims, &s.encoding).unwrap()
}

fn id_claims() -> Value {
    json!({
        "iss": ISSUER, "aud": "boggledygook", "sub": "4b0b2c6e-0000-4000-8000-000000000001",
        "iat": now(), "exp": now() + 600, "nonce": "n1", "username": "player_one", "amr": ["pwd"]
    })
}

fn accounts(s: &Signer) -> Accounts {
    Accounts::with_keys(
        Config::toddgriffin(
            "boggledygook",
            Some(String::from("secret")),
            "https://www.boggledygook.com/auth/callback",
        ),
        s.jwks.clone(),
    )
    .unwrap()
}

#[tokio::test]
async fn a_valid_id_token_verifies() {
    let s: Signer = signer();
    let claims = accounts(&s)
        .verify_id_token(&sign(&s, &id_claims(), Some("JWT")), Some("n1"))
        .await
        .unwrap();
    assert_eq!(claims.username.as_deref(), Some("player_one"));
    assert_eq!(claims.amr, vec![String::from("pwd")]);
}

#[tokio::test]
async fn wrong_audience_issuer_expiry_and_nonce_are_refused() {
    let s: Signer = signer();
    let a: Accounts = accounts(&s);
    let mut other_app: Value = id_claims();
    other_app["aud"] = json!("another-app");
    let mut other_issuer: Value = id_claims();
    other_issuer["iss"] = json!("https://evil.example/oidc");
    let mut expired: Value = id_claims();
    expired["exp"] = json!(now() - 3600);
    for (name, claims) in [
        ("audience", other_app),
        ("issuer", other_issuer),
        ("expiry", expired),
    ] {
        assert!(
            matches!(
                a.verify_id_token(&sign(&s, &claims, None), Some("n1"))
                    .await,
                Err(Error::InvalidToken(_))
            ),
            "{name}"
        );
    }
    assert!(
        a.verify_id_token(&sign(&s, &id_claims(), None), Some("other"))
            .await
            .is_err(),
        "nonce"
    );
}

#[tokio::test]
async fn unsigned_and_foreign_keys_are_refused() {
    let s: Signer = signer();
    let a: Accounts = accounts(&s);
    // alg=none.
    let none: String = format!(
        "{}.{}.",
        b64(br#"{"alg":"none","kid":"test-key"}"#),
        b64(id_claims().to_string().as_bytes())
    );
    assert!(a.verify_id_token(&none, None).await.is_err());
    // Signed by a different key under the same kid.
    let other: SecretKey = SecretKey::from_slice(&[9_u8; 32]).unwrap();
    let forged: Signer = Signer {
        encoding: EncodingKey::from_ec_der(other.to_pkcs8_der().unwrap().as_bytes()),
        jwks: s.jwks.clone(),
    };
    assert!(
        a.verify_id_token(&sign(&forged, &id_claims(), None), None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn access_tokens_need_their_type_and_scopes_read() {
    let s: Signer = signer();
    let a: Accounts = accounts(&s);
    let claims: Value = json!({
        "iss": ISSUER, "aud": "boggledygook", "client_id": "boggledygook",
        "sub": "u1", "scope": "openid friends", "iat": now(), "exp": now() + 600, "jti": "j1"
    });
    let ok = a
        .verify_access_token(&sign(&s, &claims, Some("at+jwt")))
        .await
        .unwrap();
    assert!(ok.has_scope("friends") && !ok.has_scope("friends:events"));
    // An ID token can't be passed off as an access token.
    assert!(
        a.verify_access_token(&sign(&s, &claims, Some("JWT")))
            .await
            .is_err()
    );
}

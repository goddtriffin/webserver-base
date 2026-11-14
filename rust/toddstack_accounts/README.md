# toddstack-accounts

Client for Todd Griffin's Projects accounts (`accounts.toddgriffin.me`): what a project's backend needs to sign
people in, check tokens, verify webhooks, and use friends. Part of
[toddstack](https://github.com/goddtriffin/toddstack).

Web apps use the **backend-for-frontend** pattern: your Rust backend is the OIDC client and keeps the tokens;
the browser only ever holds your own session cookie. Native apps sign in through the system browser
(`ASWebAuthenticationSession`, Custom Tabs) as public clients and send their access token to your backend,
which checks it with [`Accounts::verify_access_token`].

## Sign in (two routes)

```rust
use toddstack_accounts::{Accounts, Config, login};

let accounts = Accounts::new(Config::toddgriffin(
    "boggledygook",
    Some(client_secret),                               // from your env (1Password)
    "https://www.boggledygook.com/auth/callback",      // registered exactly
))?;

// GET /auth/login?return=/play
let (url, pending) = login::start(accounts.config(), &["openid", "profile", "offline_access", "friends"], Some("/play"));
// Store `pending` server-side or in a signed HttpOnly cookie, then redirect to `url`.

// GET /auth/callback?code=..&state=..&iss=..
let signed_in = accounts.finish_login(&pending, raw_query).await?;
// signed_in.claims.sub is the person's permanent id: key everything on it.
// Keep signed_in.tokens (refresh_token rotates: always store the new one).
// Redirect to signed_in.return_to (already checked to be a path on your site).
```

`finish_login` checks `state` and `iss`, exchanges the code with PKCE, and verifies the ID token (ES256 signature
against the published keys, issuer, audience, expiry, nonce).

| Call | What |
|---|---|
| `login::start_with(.., &Options { prompt, login_hint, max_age })` | `prompt=login` forces a fresh sign-in, `none` checks silently, `create` opens sign-up |
| `accounts.refresh(token)` | New tokens; `Error::TokenRefused` means sign in again (expired, reused, or the person removed your app) |
| `accounts.revoke(token)` | On sign-out |
| `accounts.verify_access_token(token)` | For your API: checks `typ`, issuer, audience = your client id, expiry |
| `accounts.userinfo(token)` | The profile JSON |
| `return_to::safe(path)` | Keeps redirects on your site |

## Webhooks

```rust
let event = toddstack_accounts::webhook::verify(webhook_secret, signature_header, &raw_body, now_unix)?;
match event.kind.as_str() {
    "user.deleted" => { /* delete or anonymize event.sub()'s rows */ }
    "user.updated" => { /* refresh cached username / picture */ }
    "user.signed_out" | "user.suspended" | "grant.revoked" => { /* end event.sub()'s sessions */ }
    _ => {}
}
```

Your sessions are your own: accounts can't end them. On `user.signed_out` (the person signed out everywhere,
changed or reset their password, or an admin signed them or everyone out), `user.suspended` and
`grant.revoked`, delete every session you hold for `event.sub()`. Their refresh tokens are already revoked.

Verify the raw body before parsing. Signatures older than 5 minutes are refused. Deliveries retry, so ignore
repeated `event.id`s.

## Friends and events

```rust
use toddstack_accounts::friends::{Event, Friends};

let friends = Friends::new(&accounts, &access_token);
let list = friends.list().await?;          // only friends who also use your app
friends.request_by_username("alex").await?;
friends.post_event(&Event {
    kind: String::from("beat_your_score"),
    title: String::from("Alex beat your high score"),
    body: String::from("4,200 points in today's daily"),
    collapse_key: Some(String::from("daily-2026-11-14")),
    ..Event::default()
}).await?;                                  // needs scope friends:events
```

The person can mute your app's events, one kind or all (`friends.mute("daily_completed")`,
`friends.mute(ALL_KINDS)`, `unmute`, `mutes()`), here or in Account Center.

A block overrides everything (no requests, no events either way). Events go to friends' inboxes (`inbox()`),
and will also go out as push notifications once the native apps exist.

## Not in this crate

Secrets, session storage and HTTP routes stay in your app, so the crate works with any framework.

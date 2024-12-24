import * as esbuild from "https://deno.land/x/esbuild@v0.23.0/mod.js";
import { denoPlugins } from "jsr:@luca/esbuild-deno-loader@0.10";

async function collectEntryPoints(): Promise<string[]> {
  const entryPoints: string[] = [];
  for await (const entryPoint of Deno.readDir("ui/static/script/")) {
    if (entryPoint.isFile && entryPoint.name.endsWith(".ts") && entryPoint.name !== "bundle.ts") {
      entryPoints.push(`ui/static/script/${entryPoint.name}`);
    }
  }
  return entryPoints;
}

esbuild.build({
  plugins: [...denoPlugins()],
  entryPoints: await collectEntryPoints(),
  outdir: "bin/static/script/",
  bundle: true,
  platform: "browser",
  format: "esm",
  target: "esnext",
  minify: true,
  sourcemap: true,
  treeShaking: true,
}).then(() => {
  Deno.exit(0);
}).catch(() => {
  console.error("Build failed!");
  Deno.exit(1);
});

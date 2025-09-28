/**
 * This script bundles all TypeScript files in the static/script/ directory.
 * @module
 */

import * as esbuild from "esbuild";
import { denoPlugins } from "@luca/esbuild-deno-loader";

/**
 * Collects all entry points in the static/script/ directory.
 */
async function collectEntryPoints(): Promise<string[]> {
  const entryPoints: string[] = [];
  for await (const entryPoint of Deno.readDir("static/script/")) {
    if (entryPoint.isFile && entryPoint.name.endsWith(".ts") && entryPoint.name !== "bundle.ts") {
      entryPoints.push(`static/script/${entryPoint.name}`);
    }
  }
  return entryPoints;
}

collectEntryPoints().then((entryPoints: string[]) => {
  esbuild.build({
    plugins: [...denoPlugins()],
    entryPoints: entryPoints,
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
    console.error("Failed to bundle files!");
    Deno.exit(1);
  });
}).catch(() => {
  console.error("Failed to collect entry points!");
  Deno.exit(1);
});

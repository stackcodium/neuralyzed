import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { spawnSync } from "node:child_process";
const root=resolve(import.meta.dir,".."),rust=resolve(root,"rust"),output=resolve(root,"dist/rust-wasm");
function run(command:string,args:string[],cwd=root){const result=spawnSync(command,args,{cwd,stdio:"inherit"});if(result.status!==0)throw new Error(`${command} failed with ${result.status}`)}
mkdirSync(output,{recursive:true});
const bindgen=spawnSync("wasm-bindgen",["--version"],{cwd:root,encoding:"utf8"});
if(bindgen.status!==0)throw new Error("wasm-bindgen CLI is missing. See docs/DEVELOPMENT.md for the one-time setup command.");
if(!bindgen.stdout.includes("0.2.126"))throw new Error(`Expected wasm-bindgen 0.2.126, received: ${bindgen.stdout.trim()}`);
run("cargo",["build","--manifest-path","wasm/Cargo.toml","--target","wasm32-unknown-unknown","--release","--locked"],rust);
run("wasm-bindgen",["wasm/target/wasm32-unknown-unknown/release/mib_rust_wasm.wasm","--target","web","--out-dir",output,"--remove-producers-section","--remove-name-section"],rust);
copyFileSync(resolve(rust,"assets/native-iso-atlas.png"),resolve(output,"iso-atlas.png"));
copyFileSync(resolve(rust,"assets/native-iso-atlas-meta.json"),resolve(output,"iso-atlas-meta.json"));
console.log(`Rust/WASM browser assets: ${output}`);

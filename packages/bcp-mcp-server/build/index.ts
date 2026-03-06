import { resolve } from "path";
import { concurrent_build } from "./utils";
import $bun from "./bun";
import $node from "./node";

const distDir = resolve(import.meta.dir, "../dist");

await Bun.$`rm -rf ${distDir}`;
await Bun.$`mkdir -p ${distDir}`;

await concurrent_build($bun, $node);

#!/usr/bin/env node
import { instantiate } from '../asg.js';
import { WASIShim } from '@bytecodealliance/preview2-shim/instantiation';
import fs from 'node:fs/promises';
import path from 'node:path';

// Loader for jco-transpiled component core modules
const loader = async (path) => {
  const url = new URL(`../${path}`, import.meta.url);
  const buf = await fs.readFile(url);
  return await WebAssembly.compile(buf);
};

// Collect raw args
const rawArgs = process.argv.slice(2);

// Build preopens and rewrite path-like args to guest mounts
// Strategy: for any path-like arg (relative or absolute), resolve to absolute host path,
// mount its parent directory to a unique guest mount /mN, and rewrite as /mN/<basename>.
// This avoids relying on a special root mount and works consistently across environments.
const cwd = process.cwd();
const preopens = {};
const mountMap = new Map(); // host dir -> guest mount path
let mountIndex = 0;

const args = rawArgs.map((a) => {
  // URLs should be passed through untouched
  if (a.startsWith('http://') || a.startsWith('https://')) return a;

  // Heuristics: treat .cast/.svg or anything that looks like a path as a file path
  const looksLikePath =
    a.endsWith('.cast') ||
    a.endsWith('.svg') ||
    a.startsWith('.') ||
    a.startsWith('/') ||
    a.includes(path.sep);

  if (!looksLikePath) return a;

  try {
    const abs = path.isAbsolute(a) ? a : path.resolve(cwd, a);
    const dir = path.dirname(abs);
    const base = path.basename(abs);
    let guest = mountMap.get(dir);
    if (!guest) {
      guest = `/m${mountIndex++}`;
      mountMap.set(dir, guest);
      preopens[guest] = dir;
    }
    return `${guest}/${base}`;
  } catch {
    return a;
  }
});

// Debug prints (temporary)
console.error('asg.mjs args =>', args);
console.error('asg.mjs preopens =>', preopens);

const shim = new WASIShim({
  args: ['asg', ...args],
  env: process.env,
  preopens,
});

// Instantiate and run CLI world
const component = await instantiate(loader, shim.getImportObject());

// Prefer versioned export, then unversioned, then alias
const cliWorld =
  component.run ||
  component['wasi:cli/run@0.2.3'] ||
  component['wasi:cli/run'] ||
  (typeof component.default === 'object' ? component.default : null);

if (cliWorld && typeof cliWorld.run === 'function') {
  await cliWorld.run();
} else if (typeof component.default === 'function') {
  // Some builds export the CLI entry as a default function
  await component.default();
} else {
  console.error('No CLI entrypoint found. Tried: run.run(), ["wasi:cli/run@0.2.3"].run(), ["wasi:cli/run"].run(), default().');
  process.exit(1);
}

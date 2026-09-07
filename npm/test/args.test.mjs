import assert from "node:assert/strict";
import path from "node:path";
import test from "node:test";

import { wasiArgs } from "../bin/args.mjs";

const cwd = path.resolve("recordings");

test("resolves repeated font paths in both option forms", () => {
    const absolute = path.resolve("fonts", "italic.otf");
    assert.deepEqual(wasiArgs([
        "demo.cast", "demo.svg",
        "--font-file", "fonts/Regular Face.ttf",
        "--font-file=fonts/bold.OTF",
        "--font-file", absolute,
        "--font-file", "extensionless-font",
    ], cwd), [
        "asg", path.resolve(cwd, "demo.cast"), path.resolve(cwd, "demo.svg"),
        "--font-file", path.resolve(cwd, "fonts/Regular Face.ttf"),
        `--font-file=${path.resolve(cwd, "fonts/bold.OTF")}`,
        "--font-file", absolute,
        "--font-file", path.resolve(cwd, "extensionless-font"),
    ]);
});

test("preserves URLs, stdin, and unrelated option values", () => {
    const args = [
        "https://example.com/demo.cast", "-",
        "--font-family", "Demo.ttf,monospace",
        "--font-size", "18",
        "--font-file", "./-font.ttf",
        "--font-file=--bold.ttf",
    ];
    assert.deepEqual(wasiArgs(args, cwd), [
        "asg", ...args.slice(0, 6),
        "--font-file", path.resolve(cwd, "./-font.ttf"),
        `--font-file=${path.resolve(cwd, "--bold.ttf")}`,
    ]);
});

test("leaves missing font values for the CLI to reject", () => {
    for (const args of [
        ["--font-file"],
        ["--font-file", "--no-cursor"],
        ["--font-file", ""],
        ["--font-file="],
    ]) {
        assert.deepEqual(wasiArgs(args, cwd), ["asg", ...args]);
    }
});

test("stops interpreting font options after the option terminator", () => {
    const args = ["--", "--font-file", "regular.ttf", "--font-file=bold.ttf"];
    assert.deepEqual(wasiArgs(args, cwd), ["asg", ...args]);
});

#!/usr/bin/env node

import { WASIShim } from "@bytecodealliance/preview2-shim/instantiation";
import { cli as cliShim } from "@bytecodealliance/preview2-shim";
import { wasiArgs } from "./args.mjs";

import { instantiate } from "../asg.js";

async function main() {
    // Instantiate and run CLI
    const component = await instantiate(
        void 0,
        new WASIShim({
            cli: {
                ...cliShim,
                environment: {
                    ...cliShim.environment,
                    getArguments() {
                        return wasiArgs(process.argv.slice(2), process.cwd());
                    },
                },
            },
        }).getImportObject()
    );

    component.run.run();
}

main().catch((error) => {
    console.error(error);
    process.exitCode = 1;
});

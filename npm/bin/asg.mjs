#!/usr/bin/env node

import { WASIShim } from "@bytecodealliance/preview2-shim/instantiation";
import { cli as cliShim } from "@bytecodealliance/preview2-shim";
import path from "node:path";

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
                        const args = ["asg"];
                        for (const p of process.argv.slice(2)) {
                            const isUrl = p.startsWith("http://") || p.startsWith("https://");
                            const isPath = p.endsWith(".cast") || p.endsWith(".cast.zst") || p.endsWith(".svg");
                            if (!isUrl && isPath) {
                                args.push(path.resolve(process.cwd(), p));
                                continue;
                            }

                            args.push(p);
                        }
                        return args;
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

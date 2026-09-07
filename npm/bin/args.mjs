import path from "node:path";

export function wasiArgs(argv, cwd) {
    const args = ["asg"];
    let fontFile = false;
    let options = true;
    for (const value of argv) {
        if (fontFile && value && !value.startsWith("-")) {
            args.push(path.resolve(cwd, value));
            fontFile = false;
            continue;
        }
        fontFile = false;

        if (value === "--") {
            options = false;
        }
        if (options && value.startsWith("--font-file=")) {
            const file = value.slice("--font-file=".length);
            args.push(file ? `--font-file=${path.resolve(cwd, file)}` : value);
            continue;
        }
        if (options && value === "--font-file") {
            fontFile = true;
        }

        const isUrl = value.startsWith("http://") || value.startsWith("https://");
        const isPath = value.endsWith(".cast") || value.endsWith(".cast.zst") || value.endsWith(".svg");
        args.push(!isUrl && isPath ? path.resolve(cwd, value) : value);
    }
    return args;
}

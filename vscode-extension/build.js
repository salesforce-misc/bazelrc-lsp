const esbuild = require('esbuild');
const fs = require('fs/promises');
const util = require('util');
const execFile = util.promisify(require('child_process').execFile);

const args = util.parseArgs({
    options: {
        release: { type: "boolean" },
        watch: { type: "boolean" },
    },
    strict: true,
});
const release = args.values.release;
const watch = args.values.watch;

async function build() {
    let watchers = [];
    // Cleanup
    await fs.rm("./dist", { recursive: true, force: true });
    await fs.mkdir("./dist");
    // Copy static artifacts
    await fs.copyFile('./package.json', './dist/package.json');
    await fs.copyFile('./bazelrc-language-configuration.json', './dist/bazelrc-language-configuration.json');
    await fs.copyFile('../LICENSE', './dist/LICENSE');
    await fs.copyFile('../README.md', './dist/README.md');
    // Rust build
    console.log("build rust...");
    {
        const buildArgs = release ? ["--release"] : [];
        const { stdout, stderr } = await execFile("cargo", ["build"].concat(buildArgs), { cwd: ".." });
        console.log(stdout);
        console.error(stderr);
        const outFolder = release ? "release" : "debug";
        const ext = process.platform == "win32" ? ".exe" : ""
        const src = `../target/${outFolder}/bazelrc-lsp${ext}`;
        await fs.copyFile(src, `./dist/bazelrc-lsp${ext}`);
    }
    // Typescript build
    console.log("build typescript...");
    const ctx = await esbuild.context({
        entryPoints: ['./src/extension.ts'],
        outfile: './dist/extension.js',
        platform: "node",
        format: "cjs",
        external: ["vscode"],
        bundle: true,
        minify: release,
        sourcemap: release ? false : "linked",
    });
    await ctx.rebuild();
    // Watching
    if (watch) {
        console.log("watching typescript...");
        await ctx.watch();
    } else {
        ctx.dispose();
    }
}

build()
    .catch((e) => {
        console.log(e);
        process.exit(1);
    })

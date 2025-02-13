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

function areStringArraysEqual(arr1, arr2) {
    if (arr1.length !== arr2.length) return false;
    for (let i = 0; i < arr1.length; i++) {
        if (arr1[i] !== arr2[i]) return false;
    }
    return true;
}

async function build() {
    // Rust build
    console.log("build rust...");
    const execExt = process.platform == "win32" ? ".exe" : "";
    const rustOutFolder = release ? "release" : "debug";
    const bazelrcExec = `../target/${rustOutFolder}/bazelrc-lsp${execExt}`;
    {
        const buildArgs = release ? ["--release"] : [];
        const { stdout, stderr } = await execFile("cargo", ["build"].concat(buildArgs), { cwd: ".." });
        console.log(stdout);
        console.error(stderr);
    }

    // Check if `./package.json` is up-to-date
    const packageJson = JSON.parse(await fs.readFile("./package.json"));
    console.log((await execFile(bazelrcExec, ["--version"])).stdout.trim());
    const bazelrclspVersion = (await execFile(bazelrcExec, ["--version"])).stdout.trim().match("bazelrc-lsp (\\d+.\\d+.\\d+)")[1];
    if (bazelrclspVersion != packageJson.version) {
        console.error("Error: Mismatch between package.json version and bazelrc version");
        console.error("package.json versions:", packageJson.version);
        console.error("bazelrc-lsp versions:", bazelrclspVersion);
        throw new Error("Version mismatch detected.");
    }
    const versions = packageJson
        .contributes.configuration.properties["bazelrc.bazelVersion"].enum;
    const rustVersionsJson = (await execFile(bazelrcExec, ["bazel-versions"])).stdout;
    const rustVersions = JSON.parse(rustVersionsJson);
    const expectedVersions = ["auto"].concat(rustVersions)
    if (!areStringArraysEqual(versions, expectedVersions)) {
        console.error("Error: Mismatch between supported Bazel version");
        console.error("package.json versions:", versions);
        console.error("bazelrc-lsp versions:", rustVersions);
        throw new Error("Version mismatch detected.");
    }

    // Cleanup
    await fs.rm("./dist", { recursive: true, force: true });
    await fs.mkdir("./dist");
    // Copy static artifacts
    await fs.copyFile('./package.json', './dist/package.json');
    await fs.copyFile('./bazelrc-language-configuration.json', './dist/bazelrc-language-configuration.json');
    await fs.copyFile('../LICENSE', './dist/LICENSE');
    await fs.copyFile('../README.md', './dist/README.md');
    await fs.copyFile(bazelrcExec, `./dist/bazelrc-lsp${execExt}`);
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

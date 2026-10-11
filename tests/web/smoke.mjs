// Serves a Trunk build, opens it in headless Chromium, and fails if the app panics, logs
// an error, or leaves the canvas a single flat color. Saves a screenshot of the canvas.
//
//   node tests/web/smoke.mjs --dist DIR [--screenshot PATH] [--seconds N]
import { createServer } from "node:http";
import { readFile, writeFile } from "node:fs/promises";
import { extname, join, normalize } from "node:path";
import { chromium } from "playwright";
import { PNG } from "pngjs";

const args = Object.fromEntries(
    process.argv.slice(2).reduce((pairs, arg, i, all) =>
        arg.startsWith("--") ? [...pairs, [arg.slice(2), all[i + 1]]] : pairs, [])
);
const dist = args.dist ?? "dist";
const screenshot = args.screenshot ?? "target/web-smoke.png";
const seconds = Number(args.seconds ?? 5);

const types = {
    ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm",
    ".png": "image/png", ".jpg": "image/jpeg", ".obj": "text/plain", ".css": "text/css",
};

// a static file server for the build, so assets are fetched over HTTP like a real deploy
const server = createServer(async (req, res) => {
    const path = normalize(decodeURIComponent(new URL(req.url, "http://x").pathname));
    const file = join(dist, path.endsWith("/") ? join(path, "index.html") : path);
    try {
        const body = await readFile(file);
        res.writeHead(200, { "content-type": types[extname(file)] ?? "application/octet-stream" });
        res.end(body);
    } catch {
        res.writeHead(404).end();
    }
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const url = `http://127.0.0.1:${server.address().port}/`;

// software WebGL, so this works on machines and CI runners without a GPU
const browser = await chromium.launch({ args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader"] });
const problems = [];
try {
    const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
    page.on("console", (message) => {
        if (message.type() === "error") problems.push(`console error: ${message.text()}`);
    });
    page.on("pageerror", (error) => problems.push(`page error: ${error.message}`));
    page.on("requestfailed", (request) => problems.push(`request failed: ${request.url()}`));

    await page.goto(url);
    await page.waitForSelector("canvas");
    await page.waitForTimeout(seconds * 1000);

    const png = await page.locator("canvas").screenshot();
    await writeFile(screenshot, png);
    const image = PNG.sync.read(png);
    const colors = new Set();
    for (let i = 0; i < image.data.length && colors.size < 2; i += 4) {
        colors.add(image.data.readUInt32BE(i));
    }
    if (colors.size < 2) problems.push("the canvas is a single flat color, nothing was drawn");
} finally {
    await browser.close();
    server.close();
}

console.log(`screenshot saved to ${screenshot}`);
if (problems.length > 0) {
    console.error(problems.join("\n"));
    process.exit(1);
}
console.log("web smoke test passed");

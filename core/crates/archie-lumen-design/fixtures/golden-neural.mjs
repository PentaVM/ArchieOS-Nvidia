// Regenerates the neural sphere's golden fixtures from Xavi's Lumenfield (web/src/neural-sphere, API 2).
//
//   node core/crates/archie-lumen-design/fixtures/golden-neural.mjs "<path to Lumenfield web/>"
//
// It imports the original modules themselves, unmodified (node strips their TypeScript types; three.js comes
// from that folder's node_modules). Only the outputs are committed, never the original code. The index is the
// module's own sample (neural-sphere/sample-index.json) cut to depth 5, so it keeps folded programs.
import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const web = process.argv[2];
if (!web) {
  console.error('usage: node golden-neural.mjs <Lumenfield web/>');
  process.exit(2);
}
const here = path.dirname(fileURLToPath(import.meta.url));
const load = (file) => import(pathToFileURL(path.join(web, file)).href);
const THREE = await import(pathToFileURL(path.join(web, 'node_modules/three/build/three.module.js')).href);
const { SphereModel, hash } = await load('src/neural-sphere/model.ts');
const { structuralLinks, pickBudget, nearestPairs, KIND_WEIGHT } = await load('src/neural-sphere/links.ts');
const { birthSchedule, bornProgress, smoothFactor, stepCapped, MAX_SPEED, SETTLE_DISTANCE } = await load('src/neural-sphere/motion.ts');
const { DEFAULT_SETTINGS, LIMITS, normalizeSettings } = await load('src/neural-sphere/settings.ts');
const { CATEGORIES, FOLDER, CREAM, categoryOf } = await load('src/neural-sphere/palette.ts');

const sample = JSON.parse(readFileSync(path.join(web, 'neural-sphere/sample-index.json'), 'utf8'));
const index = sample.nodes.filter((n) => n.d <= 5);
const vec = (v) => [v.x, v.y, v.z];

// The model: what is visible, folded, and where everything goes (radius 100, as the scene uses it).
const model = new SphereModel(index, {});
const modelOut = {
  vis: model.vis.map((n) => n.id),
  folded: model.vis.filter((n) => n.folded).map((n) => [n.id, n.hidden]),
  vw: model.vis.filter((n) => n.k).map((n) => [n.id, n.vw]),
  maxVD: model.maxVD,
  target: [...model.target].map(([id, v]) => [id, ...vec(v)]),
  dir: model.vis.map((n) => [n.id, ...vec(n.dir)]),
  processes: model.processes.map((p) => [p.id, p.host.id, p.name]),
  counts: model.counts(),
};
// Unfolding the first folded program, then folding everything off.
const firstFolded = model.vis.find((n) => n.folded && n.unit);
model.toggleFold(firstFolded.id);
const unfolded = { id: firstFolded.id, vis: model.vis.length, counts: model.counts() };
model.setFold(false);
const unfoldAll = { vis: model.vis.length, counts: model.counts() };

// Mesh (2) and bridge (3) links of the default layout, and the budget cut.
const fresh = new SphereModel(index, {});
const points = fresh.vis.map((n) => { const v = fresh.target.get(n.id); return { id: n.id, parent: n.p ?? '', dir: !!n.k, x: v.x, y: v.y, z: v.z, category: n.cat }; });
const links = structuralLinks(points);
const half = pickBudget(links, 0.5);
const pairs = nearestPairs(points.slice(0, 30), 2);

// Tree curves exactly as the scene draws them (SphereScene.curvePts, SEG = 10).
const curve = (P, Pdir, C, Cdir, isRoot) => {
  const len = C.distanceTo(P);
  const p1 = isRoot ? C.clone().multiplyScalar(0.3) : P.clone().addScaledVector(Pdir, len * 0.45);
  const p2 = C.clone().addScaledVector(Cdir, -len * 0.3);
  return new THREE.CubicBezierCurve3(P, p1, p2, C).getPoints(10);
};
const curves = fresh.vis.filter((n) => n.id !== '.').slice(0, 40).map((n) => {
  const P = fresh.target.get(n.p), C = fresh.target.get(n.id), parent = fresh.byId.get(n.p);
  return [n.id, curve(P, parent.dir ?? C.clone().normalize(), C, C.clone().normalize(), n.p === '.').flatMap(vec)];
});
// The bridge/content curve that sinks towards the centre (SphereScene.linkPoints).
const sink = (P, C) => {
  const chord = P.distanceTo(C), mid = P.clone().add(C).multiplyScalar(0.5);
  const off = mid.lengthSq() > 1e-9 ? mid.normalize().multiplyScalar(-0.35 * chord) : new THREE.Vector3();
  return new THREE.CubicBezierCurve3(P, P.clone().lerp(C, 1 / 3).add(off), P.clone().lerp(C, 2 / 3).add(off), C).getPoints(10);
};
const sinks = links.filter((l) => l.kind === 3).slice(0, 10).map((l) => [l.a, l.b, sink(fresh.target.get(l.a), fresh.target.get(l.b)).flatMap(vec)]);

// Colours as three.js holds them (linear, from sRGB hex) and the pseudo-random sequence of the sparks.
const colour = (hex) => { const c = new THREE.Color(hex); return [c.r, c.g, c.b]; };
const palette = Object.fromEntries([...Object.entries(CATEGORIES).map(([k, c]) => [k, colour(c.color)]), ['folders', colour(FOLDER)], ['cream', colour(CREAM)], ['hot', colour(0xffb25a)], ['holo', colour(0x5fd4ff)], ['background', colour(0x01060b)]]);
const pulses = await load('src/neural-sphere/pulses.ts').catch(() => null);
const rng = pulses ? (() => { const r = pulses.makeRng(7); return Array.from({ length: 24 }, r); })() : null;

// Motion and live build.
const birth = [...birthSchedule(fresh.vis.map((n) => ({ id: n.id, p: n.p, d: n.d })), 6)].slice(0, 60);
const cur = new THREE.Vector3(0, 0, 0), to = new THREE.Vector3(100, -40, 20);
const steps = Array.from({ length: 12 }, () => { const d = stepCapped(cur, to, smoothFactor(1 / 60, 0.18), MAX_SPEED * 100 / 60, SETTLE_DISTANCE * 100); return [d, ...vec(cur)]; });

// The layout breaks ties with String.prototype.localeCompare (ICU root collation, not code points). Every id of
// the sample plus names that exercise case, accents, digits and punctuation, in the order node sorts them.
const tricky = ['a', 'A', 'b', 'B', 'á', 'Á', 'ab', 'aB', 'a-b', 'a_b', 'a.b', 'a b', 'a/b', 'a~b', 'a$b', 'a+b', 'a(b', 'a[b', 'a@b', 'a#b', 'a%b', 'a&b', 'a,b', 'a;b', 'a!b', "a'b", 'a=b', 'a2', 'a10', 'a1', 'Z', 'z', 'ñ', 'n', 'o', 'é', 'e', 'f', '_x', '-x', '.x', '0x', 'x0', 'README.md', 'readme.md', 'Readme.md', 'src/ui', 'src-ui', 'src.ui', 'src_ui', 'srcui'];
const collation = [...new Set([...index.map((n) => n.id), ...tricky])].sort((a, b) => a.localeCompare(b));

const out = {
  origin: `Lumenfield web/src/neural-sphere (API 2), evaluated by fixtures/golden-neural.mjs with node ${process.version} and three ${THREE.REVISION}`,
  hash: ['.', 'src', 'src~', 'g0', 'g1599', 'node_modules/vite^', 'ñandú/árbol'].map((s) => [s, hash(s)]),
  categories: index.slice(0, 200).map((n) => [n.id, categoryOf(n)]),
  collation,
  model: modelOut,
  unfolded,
  unfoldAll,
  links: links.map((l) => [l.a, l.b, l.kind, l.weight]),
  budgetHalf: half.map((l) => [l.a, l.b, l.kind]),
  nearestPairs: pairs,
  kindWeight: KIND_WEIGHT,
  curves,
  sinks,
  palette,
  rng,
  birth,
  bornProgress: [[0, 1], [1, 1], [1.25, 1], [1.5, 1], [9, 1]].map(([now, at]) => bornProgress(now, at)),
  smooth: [[1 / 60, 0.18], [0.05, 0.18], [0, 0.18], [1 / 30, 1]].map(([dt, r]) => smoothFactor(dt, r)),
  steps,
  settings: {
    defaults: DEFAULT_SETTINGS,
    limits: LIMITS,
    cases: [
      { brightness: 9, nodeSize: -1, glow: Number.NaN, background: '#ABCDEF', links: { 2: { on: true, opacity: 7 } }, motion: { response: 0 } },
      { holo: 0.5, pulses: 12.6, ghosts: false, background: 'red', links: { 9: { on: true }, 1: { on: 'no', opacity: 0.4 } } },
      { linkBudget: 2, rotationSpeed: 4.5, base: 0 },
    ].map((patch) => ({ patch: JSON.parse(JSON.stringify(patch, (k, v) => (Number.isNaN(v) ? 'NaN' : v))), result: normalizeSettings(DEFAULT_SETTINGS, patch) })),
  },
};
writeFileSync(path.join(here, 'neural-index.json'), `${JSON.stringify(index)}\n`);
writeFileSync(path.join(here, 'neural.golden.json'), `${JSON.stringify(out)}\n`);
console.log(`neural golden: ${index.length} nodes, ${modelOut.vis.length} visible, ${links.length} links, rng ${rng ? 'yes' : 'no'}`);

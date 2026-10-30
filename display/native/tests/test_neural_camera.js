// Pure presentation math: no renderer, model, OS input or synthetic action receipts.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const motion = vm.createContext({});
const source = path.join(__dirname, '../qml/NeuralCameraMotion.js');
if (fs.existsSync(source)) vm.runInContext(fs.readFileSync(source, 'utf8').replace(/^\.pragma library\s*/m, ''), motion);
assert.equal(typeof motion.create, 'function', 'missing presentation camera director');
const initial = {target: {x: 0, y: 0, z: 0}, yaw: 170, elevation: 0, distance: 300};
const lookup = (x, y = 0, z = 0) => ({found: true, position: {x, y, z}});
const snapshot = s => JSON.parse(JSON.stringify(motion.pose(s)));
const near = (a, b) => assert.ok(Math.abs(a - b) < 1e-8, `${a} != ${b}`);
let count = 0;
function test(name, run) { run(); count++; console.log(`PASS ${name}`); }

test('interrupting a node transition starts from its visible midpoint', () => {
    const s = motion.create(initial, {focusMs: 1000});
    assert.ok(motion.focus(s, 'first', lookup(100), {distance: 100, yaw: -170}, false));
    motion.advance(s, 0, false); motion.advance(s, 500, false);
    const midway = snapshot(s);
    near(midway.target.x, 50); near(midway.yaw, 180);
    near(midway.distance, Math.sqrt(30000));
    motion.focus(s, 'second', lookup(-50), {}, false);
    assert.deepEqual(snapshot(s), midway);
    motion.advance(s, 1500, false);
    near(motion.pose(s).target.x, -50);
    assert.equal(s.focusId, 'second'); assert.equal(motion.moving(s), false);
});
test('missing and nonfinite targets are refused without selecting the origin', () => {
    const s = motion.create(initial);
    const before = snapshot(s);
    assert.equal(motion.focus(s, 'absent', {found: false}, {}, false), false);
    assert.equal(motion.focus(s, 'bad', lookup(NaN), {}, false), false);
    assert.deepEqual(snapshot(s), before); assert.equal(s.focusId, '');
});
test('reduced motion cuts, distance and elevation stay within camera limits', () => {
    const s = motion.create(initial, {minDistance: 40, maxDistance: 1200});
    motion.focus(s, 'node', lookup(12), {distance: 1, elevation: 100}, true);
    near(motion.pose(s).distance, 40); near(motion.pose(s).elevation, 89);
    near(motion.pose(s).target.x, 12); assert.equal(motion.moving(s), false);
    motion.focus(s, 'node', lookup(10), {distance: 9000}, false);
    motion.advance(s, 0, false); motion.advance(s, 100, true);
    near(motion.pose(s).distance, 1200); assert.equal(motion.moving(s), false);
});
test('hidden or frozen time is excluded when resuming', () => {
    const s = motion.create(initial, {focusMs: 1000});
    motion.focus(s, 'node', lookup(100), {}, false);
    motion.advance(s, 1000, false); motion.advance(s, 1250, false);
    const before = snapshot(s);
    motion.setEnabled(s, false); motion.advance(s, 9000, false);
    motion.setEnabled(s, true); motion.advance(s, 20000, false);
    assert.deepEqual(snapshot(s), before);
    motion.advance(s, 20750, false); near(motion.pose(s).target.x, 100);
});
test('manual cancellation freezes the pose until an explicit focus or home', () => {
    const s = motion.create(initial, {focusMs: 1000});
    motion.focus(s, 'node', lookup(100), {}, false);
    motion.advance(s, 0, false); motion.advance(s, 500, false);
    motion.cancel(s); const before = snapshot(s);
    motion.follow(s, 'node', lookup(200), false); motion.advance(s, 9000, false);
    assert.deepEqual(snapshot(s), before); assert.equal(s.focusId, '');
    motion.goHome(s, true); assert.deepEqual(snapshot(s), initial);
});
test('moving nodes retarget from current pose without restarting the full dive', () => {
    const s = motion.create(initial, {focusMs: 1000});
    motion.focus(s, 'node', lookup(100), {}, false);
    motion.advance(s, 0, false); motion.advance(s, 500, false);
    const before = snapshot(s);
    assert.ok(motion.follow(s, 'node', lookup(200), false));
    assert.deepEqual(snapshot(s), before);
    motion.advance(s, 1000, false); near(motion.pose(s).target.x, 200);
    motion.follow(s, 'node', {found: false}, false);
    assert.equal(s.focusId, ''); motion.advance(s, 2000, false);
    assert.deepEqual(snapshot(s), initial);
});
test('source reset clears focus and previous transition clock', () => {
    const s = motion.create(initial);
    motion.focus(s, 'private-file', lookup(20), {}, false);
    motion.advance(s, 1000, false);
    motion.reset(s, initial);
    assert.equal(s.focusId, ''); assert.equal(motion.moving(s), false);
    motion.advance(s, 100000, false); assert.deepEqual(snapshot(s), initial);
});
test('nonincreasing time never rewinds or corrupts the transition', () => {
    const s = motion.create(initial, {focusMs: 1000});
    motion.focus(s, 'node', lookup(100), {}, false);
    motion.advance(s, 1000, false); motion.advance(s, 1500, false);
    const before = snapshot(s);
    motion.advance(s, 1400, false); motion.advance(s, NaN, false);
    assert.deepEqual(snapshot(s), before);
    motion.advance(s, 2000, false); near(motion.pose(s).target.x, 100);
});
test('a real origin node is accepted while invalid camera poses leave focus intact', () => {
    const s = motion.create(initial);
    assert.ok(motion.focus(s, 'origin', lookup(0), {distance: 80}, true));
    near(motion.pose(s).target.x, 0); near(motion.pose(s).distance, 80);
    const before = snapshot(s);
    assert.equal(motion.focus(s, 'bad', lookup(40), {yaw: Infinity}, false), false);
    assert.equal(s.focusId, 'origin'); assert.deepEqual(snapshot(s), before);
    assert.equal(motion.cancel(s, {...initial, distance: NaN}), false);
    assert.deepEqual(snapshot(s), before);
});
test('frame cadence does not change the camera pose at equal elapsed time', () => {
    const a = motion.create(initial, {focusMs: 1000});
    const b = motion.create(initial, {focusMs: 1000});
    for (const s of [a,b]) {
        motion.focus(s, 'node', lookup(150, -20, 40), {yaw: -170, distance: 80}, false);
        motion.advance(s, 0, false);
    }
    for (let now=10;now<=500;now+=10) motion.advance(a,now,false);
    motion.advance(b,500,false);
    assert.deepEqual(snapshot(a),snapshot(b));
    motion.cancel(a, {...initial,target:{x:90,y:10,z:0},distance:100});
    motion.focus(a,'next',lookup(140),{},false);
    near(motion.pose(a).target.x,90);
});
console.log(`${count} neural camera contracts passed`);

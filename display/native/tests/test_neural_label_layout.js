// Screen-space label placement only. Node positions/identities stay authoritative.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const api = vm.createContext({});
const filename = path.join(__dirname, '../qml/NeuralLabelLayout.js');
if (fs.existsSync(filename)) vm.runInContext(fs.readFileSync(filename, 'utf8').replace(/^\.pragma library\s*/m, ''), api);
assert.equal(typeof api.layout, 'function', 'missing label collision layout');
const rect = (id, x = 400, y = 300) => ({id, text:id, kind:'d', anchorX:x, anchorY:y, depth:1, width:110, height:28});
const overlap = (a, b, gap = 0) => a.x < b.x+b.width+gap && a.x+a.width+gap > b.x && a.y < b.y+b.height+gap && a.y+a.height+gap > b.y;
let count = 0;
function test(name, run) { run(); count++; console.log(`PASS ${name}`); }
test('dense labels do not overlap, leave the viewport, or move their anchors', () => {
    const labels = Array.from({length:20}, (_,i) => rect(`node-${String(i).padStart(2,'0')}`,400+i%3,300+i%2));
    const placed = api.layout(labels,800,600,{gap:6});
    assert.equal(placed.length,labels.length);
    for (let i=0;i<placed.length;i++) {
        const p=placed[i], original=labels.find(x=>x.id===p.id);
        assert.equal(p.anchorX,original.anchorX); assert.equal(p.anchorY,original.anchorY);
        assert.ok(p.x>=12 && p.y>=12 && p.x+p.width<=788 && p.y+p.height<=588);
        assert.ok(!(p.anchorX>p.x-7 && p.anchorX<p.x+p.width+7 && p.anchorY>p.y-7 && p.anchorY<p.y+p.height+7),'label covers its own node');
        for(let j=i+1;j<placed.length;j++) assert.ok(!overlap(p,placed[j],5),`${p.id} overlaps ${placed[j].id}`);
    }
});
test('models, details and selection regions remain free', () => {
    const reserved=[{x:0,y:0,width:330,height:100},{x:680,y:0,width:120,height:74},{x:0,y:500,width:470,height:100}];
    const placed=api.layout([rect('models',100,40),rect('details',730,40),rect('selection',100,560)],800,600,{reserved});
    assert.equal(placed.length,3);
    for(const p of placed) for(const r of reserved) assert.ok(!overlap(p,r));
});
test('behind-camera, invalid and outside-frame nodes do not create label entities', () => {
    const placed=api.layout([rect('valid'),{...rect('behind'),depth:-1},rect('outside',-50,30),{...rect('bad'),anchorX:NaN}],800,600);
    assert.equal(placed.length,1); assert.equal(placed[0].id,'valid');
});
test('leaders end on the label boundary and start at the real projected point', () => {
    const placed=api.layout([rect('a'),rect('b')],800,600);
    for(const p of placed) {
        assert.equal(p.anchorX,400); assert.equal(p.anchorY,300);
        assert.ok(p.leaderX===p.x || p.leaderX===p.x+p.width || p.leaderY===p.y || p.leaderY===p.y+p.height);
        assert.ok(p.leaderX>=p.x && p.leaderX<=p.x+p.width && p.leaderY>=p.y && p.leaderY<=p.y+p.height);
    }
});
test('placement is deterministic and hides overflow instead of stacking it', () => {
    const labels=Array.from({length:30},(_,i)=>rect(String(i),110,70));
    const a=api.layout(labels,220,140), b=api.layout(labels,220,140);
    assert.deepEqual(JSON.parse(JSON.stringify(a)),JSON.parse(JSON.stringify(b)));
    assert.ok(a.length<labels.length);
    for(let i=0;i<a.length;i++) for(let j=i+1;j<a.length;j++) assert.ok(!overlap(a[i],a[j]));
});
test('search work and visible label count have hard bounds independent of input size', () => {
    const labels=Array.from({length:100},(_,i)=>rect(`node-${i}`));
    const report=api.plan(labels,800,600,{reserved:[{x:0,y:0,width:800,height:600}]});
    assert.equal(report.eligible,100);
    assert.equal(report.considered,24);
    assert.equal(report.labels.length,0);
    assert.equal(report.candidateChecks,24*48);
    assert.equal(report.maxCandidateChecks,48);
    assert.equal(report.overflow,100);
});
test('valid nearby slots exit early and selected nodes are considered before the cap', () => {
    const easy=api.plan([rect('a',100,100),rect('b',650,400)],800,600);
    assert.equal(easy.candidateChecks,2);
    const labels=Array.from({length:100},(_,i)=>rect(`node-${i}`));
    labels[99].selected=true;
    const report=api.plan(labels,800,600);
    assert.equal(report.labels[0].id,'node-99');
    assert.ok(report.labels.length<=24);
    assert.ok(report.candidateChecks<=24*48);
    assert.equal(report.overflow,report.eligible-report.labels.length);
});
console.log(`${count} neural label layout contracts passed`);

const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const path=require('node:path');
const tools=vm.createContext({});
vm.runInContext(fs.readFileSync(path.join(__dirname,'../qml/WorldTools.js'),'utf8').replace(/^\.pragma library\s*/,''),tools);
test('ray picks only a forward surface intersection',()=>{
 const p=tools.surfacePoint({x:0,y:0,z:60},{x:0,y:0,z:-1});
 assert.equal(p.z,20);assert.equal(tools.surfacePoint({x:0,y:0,z:60},{x:0,y:0,z:1}),null);
 assert.equal(tools.surfacePoint({x:0,y:0,z:60},{x:1,y:0,z:0}),null);
});
test('geographic axes match the native Greenwich plus-Z convention',()=>{
 assert.equal(tools.coordinates({x:20,y:0,z:0}).longitude,90);
 assert.equal(tools.coordinates({x:0,y:20,z:0}).latitude,90);
 assert.equal(tools.coordinates({x:0,y:0,z:0}),null);
});
test('geodesic distance crosses antimeridian without a long detour',()=>{
 const d=tools.distance({latitude:0,longitude:179},{latitude:0,longitude:-179});
 assert.ok(Math.abs(d-222390)<5);assert.equal(tools.distance({latitude:0,longitude:0},{latitude:0,longitude:0}),0);
 assert.equal(tools.distance({latitude:91,longitude:0},{latitude:0,longitude:0}),null);
});
test('coordinate search accepts two finite bounds-checked numbers',()=>{
 assert.equal(tools.parseCoordinates('-33.45, -70.66').longitude,-70.66);
 assert.equal(tools.parseCoordinates('0 0').latitude,0);
 for(const input of ['','Santiago','90.1 0','0 181','0 0 4','NaN, 0','1; exit,2']) assert.equal(tools.parseCoordinates(input),null);
});

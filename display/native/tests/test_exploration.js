// Run with node display/native/tests/test_exploration.js (no Qt window or daemon).
const assert=require('node:assert/strict');
const fs=require('node:fs');const vm=require('node:vm');const path=require('node:path');
const api={};vm.createContext(api);
vm.runInContext(fs.readFileSync(path.join(__dirname,'../qml/Exploration.js'),'utf8').replace('.pragma library',''),api);
assert.equal(api.bytes(null),'Sin lectura');assert.equal(api.count(undefined),'Sin lectura');
assert.equal(api.bytes(0),'0 B');assert.equal(api.count(0),'0');
assert.match(api.signalSummary({state:'partial',matched_total:0,total_is_lower_bound:true,truncated:true}),/≥0 coincidencias.*cobertura parcial/);
assert.match(api.signalSummary({state:'ready',matched_total:19,omitted_points:3,retrieved_at_ms:1792891875213}),/19 coincidencias.*3 omitidas.*Instantánea/);
assert.match(api.signalSummary({state:'stale',error:'Provider unavailable'}),/stale.*Provider unavailable/);
assert.match(api.signalSummary({points_total:96,omitted_points:44,total_is_lower_bound:true}),/≥96 coincidencias.*cobertura parcial.*44 omitidas/);
assert.match(api.timestampUtc(1792891875213),/^2026-10-25 \d{2}:\d{2}:\d{2} UTC$/);
assert.equal(api.timestampUtc(1e99),'Sin lectura');
assert.equal(api.validCamera({latitude:0,longitude:0,radius_m:1000}),true);
assert.equal(api.validCamera({latitude:91,longitude:0,radius_m:1000}),false);
assert.equal(api.validCamera({latitude:null,longitude:0,radius_m:1000}),false);
const seen=[];const jobs=[{id:'personal-job',domain:'personal'},{id:'work-job',domain:'work'}];
assert.equal(api.newJobs(jobs,seen).length,2);
assert.equal(api.newJobs(jobs,seen).length,0); // A status poll cannot reclaim a manually ceded camera.
assert.equal(api.newJobs(jobs.map(x=>({...x,state:'complete'})),seen).length,0);
assert.equal(api.newJobs([...jobs,{id:'next-job'}],seen).length,1);
assert.equal(api.selectedJob({jobs}).id,'work-job');
assert.equal(api.selectedJob({jobs,selected_job_id:'personal-job'}).id,'personal-job');
assert.equal(api.selectedJob({jobs,selected_job_id:'missing'}),null);
assert.equal(api.selectedJob({jobs,selected_job_id:null}),null);
assert.equal(api.metrics({metrics:{counters:{requests:0},gauges:{queue:2}}}).length,2);
assert.equal(api.metrics({}).length,0); // Missing metrics never become fabricated zeroes/TPS.
assert.equal(api.cameraElevation({camera:{elevation_m:4200}}),4200);
assert.equal(api.cameraElevation({camera:{elevation_m:-430}}),-430);
assert.equal(api.cameraElevation({camera:{elevation_m:0}}),0);
assert.equal(api.cameraElevation({camera:{elevation_m:null}}),null);
assert.equal(api.cameraElevation({preview:{terrain:{elevations_m:[600,700]}}}),null); // No guessed or forced measured elevation.
assert.equal(api.elevationRange({preview:{terrain:{elevations_m:[null,-430,-400]}}}),'-430.0 … -400.0 m');
const closeFrame={explorationCamera:true,cameraEye:{x:0,y:0,z:20.01},cameraTarget:{x:0,y:0,z:19.99}};
assert.equal(api.localTerrainView(closeFrame,[{kind:'terrain'}]),true); // Global ocean/basemap must yield to real below-sea geometry.
assert.equal(api.localTerrainView({...closeFrame,explorationCamera:false},[{kind:'terrain'}]),false);
assert.equal(api.localTerrainView(closeFrame,[{kind:'aoi'}]),false);
assert.equal(api.localTerrainView({...closeFrame,cameraEye:{x:0,y:0,z:60}},[{kind:'terrain'}]),false);
assert.ok(Math.abs(api.worldNearClip(closeFrame)-0.0005)<1e-12);
assert.equal(api.worldNearClip({cameraEye:{x:0,y:0,z:20},cameraTarget:{x:0,y:0,z:20}}),0.0000001);
assert.equal(api.worldNearClip({}),0.01);
for (const altitudeM of [10, 149, 1000, 100000, 1200000, 24000000]) {
    const altitude=altitudeM*20/6371008.8;
    for (const axis of ['x','y','z']) {
        const eye={x:0,y:0,z:0}; eye[axis]=20+altitude;
        const near=api.worldNearClip({cameraEye:eye,cameraTarget:{x:0,y:0,z:0}});
        assert.ok(near>0 && near<altitude*0.1,
            `globe surface at ${altitudeM}m must be beyond the near plane on ${axis}; got ${near}`);
    }
}
assert.equal(api.inference({}).tps,'Sin fuente medida publicada');
assert.equal(api.inference({metrics:{gauges:{'inference.last.effective_output_tps_available':0,'inference.last.effective_output_tokens_per_second_x1000':-1}}}).tps,'Sin fuente medida publicada');
assert.equal(api.inference({metrics:{gauges:{'inference.last.effective_output_tps_available':1,'inference.last.effective_output_tokens_per_second_x1000':12345}}}).tps,'12.35 tok/s');
console.log('Exploration projection/camera helper contracts passed');

const metric=api.beaconMetrics(false);assert.ok(metric.coreRadius<=2&&metric.haloRadius<=8&&metric.beamHeight<=24);
assert.equal(api.pickBeacon([{id:'behind',x:10,y:10,visible:false},{id:'real',x:11,y:12,visible:true}],10,10),'real');
assert.equal(api.pickBeacon([{id:'real',x:11,y:12,visible:true}],100,100),'');
assert.equal(api.pointFacingEye({x:0,y:0,z:20},{x:0,y:0,z:60}),true);
assert.equal(api.pointFacingEye({x:0,y:0,z:-20},{x:0,y:0,z:60}),false);
assert.equal(api.pointFacingEye({x:21,y:0,z:6},{x:0,y:0,z:60}),true); // Altitude above limb remains visible.
assert.equal(api.pointFacingEye({x:19,y:0,z:6},{x:0,y:0,z:60}),false);

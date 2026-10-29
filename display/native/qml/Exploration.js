.pragma library
.import "../../shell/prototype/Lang.js" as Tr
function known(value) { return typeof value === "number" && Number.isFinite(value); }
function count(value) { return known(value) ? String(value) : Tr.t("Sin lectura"); }
function timestampUtc(value) {
    if(!known(value)) return Tr.t("Sin lectura");
    var date=new Date(value);
    return Number.isFinite(date.getTime()) ? date.toISOString().slice(0,19).replace("T"," ")+" UTC" : Tr.t("Sin lectura");
}
function signalSummary(source) {
    var parts=[source.state || Tr.t("Instantánea")];
    var total=known(source.matched_total)?source.matched_total:source.points_total;
    if(known(total)) parts.push((source.total_is_lower_bound?"≥":"")+total+Tr.t(" coincidencias"));
    if(source.total_is_lower_bound || source.truncated) parts.push(Tr.t("cobertura parcial"));
    if(known(source.omitted_points) && source.omitted_points>0) parts.push(source.omitted_points+Tr.t(" omitidas"));
    if(known(source.retrieved_at_ms)) parts.push(Tr.t("Instantánea: ")+timestampUtc(source.retrieved_at_ms));
    if(source.error) parts.push(source.error);
    return parts.join(" · ");
}
function bytes(value) {
    if (!known(value)) return Tr.t("Sin lectura");
    var n=Number(value);
    return n < 1024 ? n + " B" : n < 1048576 ? (n/1024).toFixed(1)+" KiB" : (n/1048576).toFixed(1)+" MiB";
}
function state(value) {
    return ({queued:Tr.t("En cola"),running:Tr.t("Explorando"),waiting_rate_limit:Tr.t("Esperando cuota"),paused:Tr.t("En pausa"),complete:Tr.t("Completo"),failed:"Error",cancelled:Tr.t("Cancelado")})[value] || Tr.t("Sin lectura");
}
function tone(value) { return value==="failed" ? "bad" : value==="waiting_rate_limit" || value==="paused" ? "warn" : value==="complete" ? "ok" : "neutral"; }
function validCamera(camera) {
    return !!camera && known(camera.latitude) && Math.abs(camera.latitude)<=90 && known(camera.longitude) && Math.abs(camera.longitude)<=180 && known(camera.radius_m) && camera.radius_m>0 && camera.radius_m<=6371000;
}
function newJobs(jobs, seen) {
    var added=[];
    for(var i=0;i<jobs.length;++i) {
        var id=jobs[i].id;
        if(typeof id!=="string" || !id.length || seen.indexOf(id)>=0) continue;
        seen.push(id);added.push(jobs[i]);
    }
    return added;
}
function selectedJob(projection) {
    var jobs=projection.jobs || [];
    if(typeof projection.selected_job_id==="undefined") return jobs.length?jobs[jobs.length-1]:null;
    return jobs.find(function(job){return job.id===projection.selected_job_id}) || null;
}
function metrics(snapshot) {
    var rows=[], registry=snapshot.metrics || {};
    ["counters","gauges"].forEach(function(kind) {
        var values=registry[kind] || {};
        Object.keys(values).sort().forEach(function(key) {rows.push({name:key, value:String(values[key]), kind:kind});});
    });
    return rows;
}
function cameraElevation(job) {
    var value=job.camera && job.camera.elevation_m;
    return known(value) && value>=-12000 && value<=12000 ? value : null;
}
function elevationRange(job) {
    var samples=job.preview && job.preview.terrain && job.preview.terrain.elevations_m;
    var values=(samples || []).filter(function(value) { return known(value) && value>=-12000 && value<=12000; });
    return values.length ? Math.min.apply(null,values).toFixed(1)+" … "+Math.max.apply(null,values).toFixed(1)+" m" : Tr.t("Sin lectura");
}
function localTerrainView(frame, meshes) {
    if(!frame || frame.explorationCamera!==true || !frame.cameraEye || !frame.cameraTarget) return false;
    var eye=frame.cameraEye,target=frame.cameraTarget;
    var distance=Math.hypot(eye.x-target.x,eye.y-target.y,eye.z-target.z);
    // At close AOI scale the decoded patch replaces the global basemap, including ocean.
    return Number.isFinite(distance) && distance<=0.5 && (meshes || []).some(function(mesh) { return mesh.kind==="terrain"; });
}
function worldNearClip(frame) {
    if(!frame || !frame.cameraEye || !frame.cameraTarget) return 0.01;
    var eye=frame.cameraEye,target=frame.cameraTarget;
    var distance=Math.hypot(eye.x-target.x,eye.y-target.y,eye.z-target.z);
    var altitude=Math.max(0,Math.hypot(eye.x,eye.y,eye.z)-20);
    // A globe camera looks at Earth's centre. Its target distance stays near
    // one Earth radius even metres above the ground; clipping must use altitude.
    return Number.isFinite(distance) && Number.isFinite(altitude)
        ? Math.max(0.0000001,Math.min(distance,altitude)*0.05) : 0.01;
}

function inference(snapshot) {
    var gauges=snapshot.metrics && snapshot.metrics.gauges || {};
    var prefix="inference.last.";
    var tps=gauges[prefix+"effective_output_tokens_per_second_x1000"];
    var tokens=gauges[prefix+"output_tokens"];
    return {
        tps:gauges[prefix+"effective_output_tps_available"]===1 && known(tps) && tps>=0 ? (tps/1000).toFixed(2)+" tok/s" : Tr.t("Sin fuente medida publicada"),
        tokens:gauges[prefix+"output_tokens_available"]===1 && known(tokens) && tokens>=0 ? String(tokens) : Tr.t("Sin lectura"),
        elapsed:gauges[prefix+"request_elapsed_ms_available"]===1 && known(gauges[prefix+"request_elapsed_ms"]) && gauges[prefix+"request_elapsed_ms"]>=0 ? gauges[prefix+"request_elapsed_ms"]+" ms" : Tr.t("Sin lectura"),
        observed:known(gauges[prefix+"observed_at_ms"]) ? new Date(gauges[prefix+"observed_at_ms"]).toLocaleTimeString() : Tr.t("Sin lectura")
    };
}

function beaconMetrics(selected) { return {coreRadius:selected ? 2 : 1.6, haloRadius:selected ? 8 : 6, beamHeight:selected ? 22 : 14, pickRadius:8}; }
function pickBeacon(points,x,y) {var best="",distance=64;points.forEach(function(point){var d=(point.x-x)*(point.x-x)+(point.y-y)*(point.y-y);if(point.visible && d<=distance){distance=d;best=point.id;}});return best;}
function pointFacingEye(position,eye) {
    if(!position || !eye) return false;
    // Test the eye-to-observation segment against the reference globe. Elevated
    // aircraft can remain visible beyond a surface point's tangent horizon.
    var dx=position.x-eye.x,dy=position.y-eye.y,dz=position.z-eye.z;
    var length2=dx*dx+dy*dy+dz*dz;
    if(!(length2>0)) return false;
    var t=-(eye.x*dx+eye.y*dy+eye.z*dz)/length2;
    if(t<=0 || t>=1) return true;
    var x=eye.x+t*dx,y=eye.y+t*dy,z=eye.z+t*dz;
    var radius2=Math.min(400,position.x*position.x+position.y*position.y+position.z*position.z);
    return x*x+y*y+z*z>=radius2-0.00000001;
}

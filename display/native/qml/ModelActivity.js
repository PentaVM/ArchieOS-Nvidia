.pragma library

// Core completion receipts identify models. Never attribute a global last rate
// to a catalog entry or describe a completed request as current generation.
function fromSnapshot(snapshot, nowMs, connected) {
    if (connected !== true || !snapshot || snapshot.source !== "live" || snapshot.daemon_reachable !== true
            || !Number.isFinite(nowMs)) return null
    const source=snapshot.metrics && snapshot.metrics.model_inference
    if (!Array.isArray(source)) return null
    const seen=Object.create(null), rows=[]
    source.slice().sort(function(a,b) {
        return Number(b && b.observed_at_ms || 0)-Number(a && a.observed_at_ms || 0)
    }).forEach(function(row) {
        if (!row || typeof row.model_id !== "string" || !row.model_id.length
                || row.model_id.length>128 || /[\s\x00-\x1f\x7f]/.test(row.model_id)
                || row.state!=="recent_completed" || seen[row.model_id]
                || !Number.isSafeInteger(row.observed_at_ms) || row.observed_at_ms<=0
                || row.observed_at_ms>nowMs+1000 || !Number.isSafeInteger(row.expires_at_ms)
                || row.expires_at_ms<=nowMs || row.expires_at_ms<=row.observed_at_ms
                || row.expires_at_ms-row.observed_at_ms>30000) return
        seen[row.model_id]=true
        const measured=Number.isSafeInteger(row.effective_output_tps_x1000)
            && row.effective_output_tps_x1000>=0 && Number.isSafeInteger(row.output_tokens)
            && row.output_tokens>=0 && Number.isSafeInteger(row.request_elapsed_ms)
            && row.request_elapsed_ms>0
        rows.push({id:row.model_id,label:row.model_id,active:true,stale:false,
            state:"reciente",tps_available:measured,
            tps:measured ? row.effective_output_tps_x1000/1000 : null})
    })
    return rows.slice(0,6)
}

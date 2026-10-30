.pragma library

// Screen-space annotation layout only. Anchors are untouched projections from the
// native field; a label may move, never the node it names. No synthetic IDs/rows.
function finite(v) { return typeof v === "number" && isFinite(v) }
function clamp(v, low, high) { return Math.max(low, Math.min(high, v)) }
function overlaps(a, b, gap) {
    return a.x < b.x+b.width+gap && a.x+a.width+gap > b.x
        && a.y < b.y+b.height+gap && a.y+a.height+gap > b.y
}
function leader(rect, anchorX, anchorY) {
    var x=clamp(anchorX,rect.x,rect.x+rect.width), y=clamp(anchorY,rect.y,rect.y+rect.height)
    if (x>rect.x && x<rect.x+rect.width && y>rect.y && y<rect.y+rect.height) {
        var distances=[x-rect.x,rect.x+rect.width-x,y-rect.y,rect.y+rect.height-y]
        var side=distances.indexOf(Math.min.apply(null,distances))
        if(side===0) x=rect.x
        else if(side===1) x=rect.x+rect.width
        else if(side===2) y=rect.y
        else y=rect.y+rect.height
    }
    return {x:x,y:y}
}
// Hard bounds apply to every camera frame; no unbounded search or timer lag.
var MAX_LABELS = 24
var MAX_CANDIDATES = 48
var candidateOffsets = (function() {
    var offsets=[]
    for(var x=-3;x<=3;x++) for(var y=-3;y<=3;y++) offsets.push({x:x,y:y})
    offsets.sort(function(a,b) {
        return (a.x*a.x+a.y*a.y)-(b.x*b.x+b.y*b.y) || a.y-b.y || a.x-b.x
    })
    return offsets.slice(0,MAX_CANDIDATES)
})()
function plan(labels, width, height, options) {
    options=options || {}
    var report={labels:[],total:Array.isArray(labels) ? labels.length : 0,
        eligible:0,considered:0,overflow:0,candidateChecks:0,maxCandidateChecks:0}
    if(!finite(width) || !finite(height) || width<48 || height<48) return report
    var gap=finite(options.gap) ? Math.max(0,options.gap) : 6
    var margin=12, reserved=options.reserved || [], placed=report.labels
    var ordered=(Array.isArray(labels) ? labels : []).filter(function(label) {
        return label && typeof label.id==="string" && label.id.length>0
            && finite(label.anchorX) && finite(label.anchorY) && finite(label.depth) && label.depth>0
            && label.anchorX>=0 && label.anchorX<=width && label.anchorY>=0 && label.anchorY<=height
            && finite(label.width) && label.width>0 && finite(label.height) && label.height>0
    }).slice().sort(function(a,b) {
        var aPriority=a.selected===true ? 0 : a.kind==="p" ? 1 : 2
        var bPriority=b.selected===true ? 0 : b.kind==="p" ? 1 : 2
        return aPriority-bPriority || (a.id<b.id ? -1 : a.id>b.id ? 1 : 0)
    })
    report.eligible=ordered.length
    var selected=ordered.slice(0,MAX_LABELS)
    report.considered=selected.length
    selected.forEach(function(label) {
        var w=Math.min(label.width,width-margin*2), h=label.height
        if(h>height-margin*2) return
        var found=null, checks=0
        for(var i=0;i<candidateOffsets.length;i++) {
            checks++; report.candidateChecks++
            var offset=candidateOffsets[i]
            var rect={x:clamp(label.anchorX-w/2+offset.x*(w+gap),margin,width-margin-w),
                y:clamp(label.anchorY-h-12+offset.y*(h+gap),margin,height-margin-h),width:w,height:h}
            // Leave the actual marker clear, including its rim.
            if(label.anchorX>rect.x-8 && label.anchorX<rect.x+w+8
                && label.anchorY>rect.y-8 && label.anchorY<rect.y+h+8) continue
            if(reserved.some(function(other){return overlaps(rect,other,gap)})
                || placed.some(function(other){return overlaps(rect,other,gap)})) continue
            found=rect
            break // nearest viable slot; never score every candidate after success
        }
        report.maxCandidateChecks=Math.max(report.maxCandidateChecks,checks)
        if(!found) return
        var end=leader(found,label.anchorX,label.anchorY)
        placed.push({id:label.id,text:label.text,kind:label.kind,x:found.x,y:found.y,
            width:found.width,height:found.height,anchorX:label.anchorX,anchorY:label.anchorY,
            leaderX:end.x,leaderY:end.y})
    })
    report.overflow=report.eligible-placed.length
    return report
}
function layout(labels,width,height,options) { return plan(labels,width,height,options).labels }

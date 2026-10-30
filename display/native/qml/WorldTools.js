.pragma library
function valid(point) {
    return point && Number.isFinite(point.latitude) && Math.abs(point.latitude)<=90
        && Number.isFinite(point.longitude) && Math.abs(point.longitude)<=180;
}
function coordinates(p) {
    var length=Math.hypot(p.x,p.y,p.z);
    if(!Number.isFinite(length) || length<=0) return null;
    return {latitude:Math.asin(Math.max(-1,Math.min(1,p.y/length)))*180/Math.PI,
            longitude:Math.atan2(p.x,p.z)*180/Math.PI};
}
function surfacePoint(origin,direction) {
    var a=direction.x*direction.x+direction.y*direction.y+direction.z*direction.z;
    var b=2*(origin.x*direction.x+origin.y*direction.y+origin.z*direction.z);
    var c=origin.x*origin.x+origin.y*origin.y+origin.z*origin.z-400;
    var discriminant=b*b-4*a*c;
    if(!Number.isFinite(discriminant) || !(a>0) || discriminant<0) return null;
    var t=(-b-Math.sqrt(discriminant))/(2*a);
    if(t<0) t=(-b+Math.sqrt(discriminant))/(2*a);
    if(t<0 || !Number.isFinite(t)) return null;
    return {x:origin.x+t*direction.x,y:origin.y+t*direction.y,z:origin.z+t*direction.z};
}
function distance(a,b) {
    if(!valid(a) || !valid(b)) return null;
    var dlat=(b.latitude-a.latitude)*Math.PI/180,dlon=(b.longitude-a.longitude)*Math.PI/180;
    var h=Math.sin(dlat/2)**2+Math.cos(a.latitude*Math.PI/180)*Math.cos(b.latitude*Math.PI/180)*Math.sin(dlon/2)**2;
    return 6371008.8*2*Math.asin(Math.sqrt(Math.max(0,Math.min(1,h))));
}
function parseCoordinates(text) {
    var values=text.trim().split(/\s*,\s*|\s+/);
    if(values.length!==2 || !values.every(function(v){return /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)$/.test(v)})) return null;
    var p={latitude:Number(values[0]),longitude:Number(values[1])};
    return valid(p) ? p : null;
}
function formatDistance(metres) {
    return metres===null ? "" : metres<1000 ? metres.toFixed(1)+" m" : (metres/1000).toFixed(2)+" km";
}

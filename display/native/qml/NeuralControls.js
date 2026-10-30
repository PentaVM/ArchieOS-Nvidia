.pragma library

// Calibrate the shared desktop controls around the delivered neural look (1.0).
// Requested activity is distinct from the governor's particle ceiling.
function settings(source) {
    return ({brightness:Number(source.exposure === undefined ? 2.2 : source.exposure)/2.2,
        rotationSpeed:source.rotate === false ? 0 : Number(source.rotationSpeed === undefined ? .36 : source.rotationSpeed)/.36,
        pulseSpeed:Number(source.pulseSpeed === undefined ? .33 : source.pulseSpeed)/.33,
        requestedPulses:source.pulses === false ? 0 : 70,
        ghosts:source.ambient !== false,
        links:source.links !== false, shell:source.shell !== false, bloom:source.bloom !== false,
        visibleLimit:Number(source.visibleLimit === undefined ? 420 : source.visibleLimit)})
}

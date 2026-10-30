import QtQuick
import QtTest
import "../../native/qml/NeuralControls.js" as Controls
TestCase {
    name:"NeuralControls"
    function test_overlay_requests_are_forwarded() {
        const settings=Controls.settings({links:false,shell:false,bloom:false})
        compare(settings.links,false);compare(settings.shell,false);compare(settings.bloom,false)
    }
    function test_default_grade_preserves_the_delivered_brightness() {
        compare(Controls.settings({exposure:2.2,rotationSpeed:.36,rotate:true,pulseSpeed:.33,pulses:true,ambient:true}).brightness,1)
    }
    function test_disabled_motion_and_activity_are_forwarded() {
        const settings=Controls.settings({exposure:1.6,rotate:false,pulses:false,ambient:false,pulseSpeed:.2})
        compare(settings.rotationSpeed,0);compare(settings.requestedPulses,0);compare(settings.ghosts,false)
        verify(settings.brightness<1)
    }
    function test_explicit_speed_and_density_reach_the_projection() {
        const settings=Controls.settings({exposure:2.5,rotationSpeed:.72,rotate:true,pulseSpeed:.66,pulses:true,ambient:true,visibleLimit:60})
        compare(settings.rotationSpeed,2);compare(settings.pulseSpeed,2);compare(settings.visibleLimit,60)
        compare(settings.requestedPulses,70);verify(settings.brightness>1)
    }
}

import QtQuick
import QtTest
import "../../native/qml/OrbActions.js" as Actions
TestCase {
    name:"OrbActionResults"
    function test_unknown_node_never_changes_selection() {
        var changed=false
        var controller={containsNode:function(id){return false},select:function(id){changed=true}}
        verify(Actions.perform("focus",{id:"missing"},controller,null,null,null).length>0)
        compare(changed,false)
    }
    function test_existing_node_selection_is_applied() {
        var selected=""
        var controller={containsNode:function(id){return id==="known"},select:function(id){selected=id}}
        compare(Actions.perform("focus",{id:"known"},controller,null,null,null),"")
        compare(selected,"known")
    }
    function test_neural_failure_and_empty_pick_are_propagated() {
        var neural={focusNode:function(id){return false},clickAt:function(x,y){return false},width:100,height:100}
        verify(Actions.perform("focus",{id:"missing"},null,null,null,neural).length>0)
        verify(Actions.perform("pointer",{x:0.5,y:0.5,click:true},null,null,null,neural).length>0)
    }
}

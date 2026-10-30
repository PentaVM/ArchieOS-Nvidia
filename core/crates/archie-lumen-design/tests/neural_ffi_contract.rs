//! Host-facing regressions: exercise the same length-query/fill contract as Qt.
use archie_lumen_design::neural::ffi::*;
use std::ffi::CStr;
use std::ptr;

#[test]
fn a_picked_id_can_be_queried_before_allocating_its_buffer() {
    unsafe {
        let root = env!("CARGO_MANIFEST_DIR").as_bytes();
        let index = neural_scan(root.as_ptr(), root.len(), 1, 1);
        assert!(!index.is_null());
        let sphere = neural_new(index, 7, true);
        neural_index_free(index);
        assert!(!sphere.is_null());
        let origin = [0.0, 0.0, 300.0];
        let direction = [0.0, 0.0, -1.0];
        let length = neural_pick(
            sphere,
            origin.as_ptr(),
            direction.as_ptr(),
            3.0,
            ptr::null_mut(),
            0,
        );
        assert_eq!(
            length, 1,
            "the root '.' must survive the host's initial length query"
        );
        let mut output = vec![0; length + 1];
        assert_eq!(
            neural_pick(
                sphere,
                origin.as_ptr(),
                direction.as_ptr(),
                3.0,
                output.as_mut_ptr(),
                output.len()
            ),
            length
        );
        assert_eq!(CStr::from_ptr(output.as_ptr()).to_bytes(), b".");
        let miss = [1.0, 0.0, 0.0];
        assert_eq!(
            neural_pick(
                sphere,
                origin.as_ptr(),
                miss.as_ptr(),
                3.0,
                ptr::null_mut(),
                0
            ),
            0
        );
        neural_free(sphere);
    }
}

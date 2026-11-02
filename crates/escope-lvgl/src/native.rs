//! Optional access to the application's existing `rs_lvgl` native provider.
//! This module registers no display, input device, executor or second LVGL.
use crate::{TreeSource, UiNode};
use core::{marker::PhantomData, ptr::NonNull};
use escope_protocol::Status;
use rs_lvgl::sys;

/// A bounded native tree walk. All access remains on the constructing thread.
/// Call changed after updates and before serving the next tree request.
pub struct NativeTree<const N: usize> {
    root: NonNull<sys::lv_obj_t>,
    revision: u32,
    _thread: PhantomData<*mut ()>,
}

impl<const N: usize> NativeTree<N> {
    /// # Safety
    /// `root` must remain a live LVGL object until this tree is dropped. Create,
    /// read, mutate and drop this adapter on the LVGL thread. No LVGL mutation,
    /// deinitialization or callback reentry may overlap a read_node call.
    pub unsafe fn new(root: NonNull<sys::lv_obj_t>) -> Self {
        Self {
            root,
            revision: 1,
            _thread: PhantomData,
        }
    }
    /// Invalidate client cursors after the application changes its native tree.
    /// A session must be replaced if the revision space is exhausted.
    pub fn changed(&mut self) -> Result<(), Status> {
        self.revision = self.revision.checked_add(1).ok_or(Status::Capacity)?;
        Ok(())
    }
}

impl<const N: usize> TreeSource for NativeTree<N> {
    fn revision(&self) -> u32 {
        self.revision
    }
    fn read_node(&mut self, cursor: u32) -> Result<UiNode<'_>, Status> {
        if N == 0 {
            return Err(Status::Capacity);
        }
        let mut objects = [core::ptr::null_mut(); N];
        let mut parents = [0u32; N];
        objects[0] = self.root.as_ptr();
        let mut count = 1;
        let mut index = 0;
        // SAFETY: constructor's contract establishes exclusive, live LVGL
        // access. Breadth-first enumeration stores at most N live pointers.
        unsafe {
            while index < count {
                let children = sys::lv_obj_get_child_count(objects[index]);
                for child_index in 0..children {
                    if count == N {
                        return Err(Status::Capacity);
                    }
                    let child_index = i32::try_from(child_index).map_err(|_| Status::Capacity)?;
                    let child = sys::lv_obj_get_child(objects[index], child_index);
                    if child.is_null() {
                        return Err(Status::Stale);
                    }
                    objects[count] = child;
                    parents[count] = index as u32 + 1;
                    count += 1;
                }
                index += 1;
            }
            let cursor = cursor as usize;
            if cursor >= count {
                return Err(Status::InvalidArgument);
            }
            let object = objects[cursor];
            let mut area = core::mem::zeroed::<sys::lv_area_t>();
            sys::lv_obj_get_coords(object, &mut area);
            Ok(UiNode {
                // IDs are valid only with this response's revision/session.
                id: cursor as u32 + 1,
                parent: parents[cursor],
                kind: "lv_obj",
                name: "",
                x: area.x1,
                y: area.y1,
                width: u32::try_from((area.x2 as i64 - area.x1 as i64 + 1).max(0))
                    .map_err(|_| Status::Internal)?,
                height: u32::try_from((area.y2 as i64 - area.y1 as i64 + 1).max(0))
                    .map_err(|_| Status::Internal)?,
                flags: sys::lv_obj_get_state(object),
                revision: self.revision,
                next_cursor: (cursor + 1 < count).then_some(cursor as u32 + 1),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UiBridge;
    use escope_protocol::{decode_payload, encode_payload, TreeQuery};

    #[test]
    fn reads_real_native_tree_and_rejects_old_revision() {
        rs_lvgl::init();
        let display = rs_lvgl::Display::register::<_, { 64 * 64 }>(64, 64, |_| {}).unwrap();
        // SAFETY: this test has exclusive LVGL access and tears down the
        // adapter before display deletion/deinitialization.
        unsafe {
            let screen = NonNull::new(sys::lv_screen_active()).unwrap();
            let child = sys::lv_obj_create(screen.as_ptr());
            sys::lv_obj_set_pos(child, 3, 4);
            sys::lv_obj_set_size(child, 20, 18);
            sys::lv_obj_update_layout(screen.as_ptr());
            {
                let tree = NativeTree::<16>::new(screen);
                let mut bridge = UiBridge::new(tree);
                let mut request = [0; 64];
                let mut reply = [0; 1024];
                let n = encode_payload(
                    &TreeQuery {
                        cursor: 1,
                        revision: 1,
                    },
                    &mut request,
                )
                .unwrap();
                let size = bridge.tree(&request[..n], &mut reply).unwrap();
                let node: UiNode<'_> = decode_payload(&reply[..size]).unwrap();
                assert_eq!((node.parent, node.width, node.height), (1, 20, 18));
                sys::lv_obj_delete(child);
                bridge.source_mut().changed().unwrap();
                assert_eq!(bridge.tree(&request[..n], &mut reply), Err(Status::Stale));
            }
            display.delete();
            rs_lvgl::deinit();
        }
    }
}

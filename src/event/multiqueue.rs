//! Translated from `src/nvim/event/multiqueue.c` in full.

use std::collections::VecDeque;

use crate::event::defs::{Event, PutCallback};

enum MultiQueueItem {
    Link {
        child: *mut MultiQueue,
        id: u64,
    },
    Event {
        event: Event,
        parent_link_id: Option<u64>,
    },
}

/// Multi-level event queue (`MultiQueue`).
pub struct MultiQueue {
    parent: *mut MultiQueue,
    items: VecDeque<MultiQueueItem>,
    on_put: Option<PutCallback>,
    data: *mut std::ffi::c_void,
    size: usize,
    next_link_id: u64,
}

fn _multiqueue_new(
    parent: *mut MultiQueue,
    on_put: Option<PutCallback>,
    data: *mut std::ffi::c_void,
) -> *mut MultiQueue {
    Box::into_raw(Box::new(MultiQueue {
        parent,
        items: VecDeque::new(),
        on_put,
        data,
        size: 0,
        next_link_id: 0,
    }))
}

/// Create a parentless queue (`multiqueue_new`).
#[must_use]
pub fn multiqueue_new(
    on_put: Option<PutCallback>,
    data: *mut std::ffi::c_void,
) -> *mut MultiQueue {
    _multiqueue_new(std::ptr::null_mut(), on_put, data)
}

/// Create a child queue (`multiqueue_new_child`).
///
/// # Safety
/// `parent` must be a valid root queue that outlives the child.
#[must_use]
pub unsafe fn multiqueue_new_child(
    parent: *mut MultiQueue,
) -> *mut MultiQueue {
    assert!(!parent.is_null());
    assert!(unsafe { (*parent).parent }.is_null());
    unsafe { (*parent).size += 1 };
    _multiqueue_new(parent, None, std::ptr::null_mut())
}

/// Free a queue and detach its pending child links
/// (`multiqueue_free`).
///
/// # Safety
/// `queue` must come from a multiqueue constructor and be freed once.
pub unsafe fn multiqueue_free(queue: *mut MultiQueue) {
    assert!(!queue.is_null());
    let mut queue = unsafe { Box::from_raw(queue) };
    if !queue.parent.is_null() {
        for item in &queue.items {
            if let MultiQueueItem::Event {
                parent_link_id: Some(id),
                ..
            } = item
            {
                let parent = unsafe { &mut *queue.parent };
                if let Some(position) = parent.items.iter().position(
                    |item| matches!(
                        item,
                        MultiQueueItem::Link { id: candidate, .. }
                            if candidate == id
                    ),
                ) {
                    parent.items.remove(position);
                }
            }
        }
    }
    queue.items.clear();
}

fn multiqueue_remove(queue: &mut MultiQueue) -> Event {
    let item = queue.items.pop_front().expect("queue is nonempty");
    let event = match item {
        MultiQueueItem::Event {
            event,
            parent_link_id,
        } => {
            if let Some(id) = parent_link_id
                && !queue.parent.is_null()
            {
                let parent = unsafe { &mut *queue.parent };
                let position = parent
                    .items
                    .iter()
                    .position(|item| {
                        matches!(
                            item,
                            MultiQueueItem::Link {
                                id: candidate,
                                ..
                            } if *candidate == id
                        )
                    })
                    .expect("child event has a parent link");
                parent.items.remove(position);
            }
            event
        }
        MultiQueueItem::Link { child, id } => {
            let child = unsafe { &mut *child };
            let position = child
                .items
                .iter()
                .position(|item| {
                    matches!(
                        item,
                        MultiQueueItem::Event {
                            parent_link_id: Some(candidate),
                            ..
                        } if *candidate == id
                    )
                })
                .expect("parent link has a child event");
            let MultiQueueItem::Event { event, .. } =
                child.items.remove(position).unwrap()
            else {
                unreachable!()
            };
            event
        }
    };
    queue.size = queue.size.wrapping_sub(1);
    event
}

/// Remove and return the next event (`multiqueue_get`).
///
/// # Safety
/// `queue` must be valid and exclusively accessible.
#[must_use]
pub unsafe fn multiqueue_get(queue: *mut MultiQueue) -> Event {
    let queue = unsafe { &mut *queue };
    if queue.items.is_empty() {
        Event::default()
    } else {
        multiqueue_remove(queue)
    }
}

/// Enqueue one event (`multiqueue_put_event`).
///
/// # Safety
/// `queue` and its parent, if any, must be valid and exclusively
/// accessible.
pub unsafe fn multiqueue_put_event(
    queue: *mut MultiQueue,
    event: Event,
) {
    assert!(!queue.is_null());
    let queue_ref = unsafe { &mut *queue };
    let parent_link_id = if queue_ref.parent.is_null() {
        None
    } else {
        let id = queue_ref.next_link_id;
        queue_ref.next_link_id = queue_ref.next_link_id.wrapping_add(1);
        unsafe { &mut *queue_ref.parent }
            .items
            .push_back(MultiQueueItem::Link { child: queue, id });
        Some(id)
    };
    queue_ref.items.push_back(MultiQueueItem::Event {
        event,
        parent_link_id,
    });
    queue_ref.size += 1;
    if !queue_ref.parent.is_null() {
        let parent = unsafe { &mut *queue_ref.parent };
        if let Some(on_put) = parent.on_put {
            unsafe { on_put(queue_ref.parent, parent.data) };
        }
    }
}

/// Move every event from `source` to `destination`
/// (`multiqueue_move_events`).
///
/// # Safety
/// Both queues must be valid, distinct, and exclusively accessible.
pub unsafe fn multiqueue_move_events(
    destination: *mut MultiQueue,
    source: *mut MultiQueue,
) {
    while !unsafe { multiqueue_empty(source) } {
        let event = unsafe { multiqueue_get(source) };
        unsafe { multiqueue_put_event(destination, event) };
    }
}

/// Invoke and remove every queued event
/// (`multiqueue_process_events`).
///
/// # Safety
/// The queue must be valid and event callback contracts must hold.
pub unsafe fn multiqueue_process_events(queue: *mut MultiQueue) {
    while !unsafe { multiqueue_empty(queue) } {
        let mut event = unsafe { multiqueue_get(queue) };
        if let Some(handler) = event.handler {
            unsafe { handler(event.argv.as_mut_ptr()) };
        }
    }
}

/// Remove all events without invoking them
/// (`multiqueue_purge_events`).
///
/// # Safety
/// `queue` must be valid and exclusively accessible.
pub unsafe fn multiqueue_purge_events(queue: *mut MultiQueue) {
    while !unsafe { multiqueue_empty(queue) } {
        let _ = unsafe { multiqueue_get(queue) };
    }
}

/// Whether the queue has no pending nodes (`multiqueue_empty`).
///
/// # Safety
/// `queue` must be valid.
#[must_use]
pub unsafe fn multiqueue_empty(queue: *const MultiQueue) -> bool {
    assert!(!queue.is_null());
    unsafe { (*queue).items.is_empty() }
}

/// Replace the parent of an empty queue
/// (`multiqueue_replace_parent`).
///
/// # Safety
/// Both queue pointers must remain valid for the resulting hierarchy.
pub unsafe fn multiqueue_replace_parent(
    queue: *mut MultiQueue,
    new_parent: *mut MultiQueue,
) {
    assert!(unsafe { multiqueue_empty(queue) });
    unsafe { (*queue).parent = new_parent };
}

/// Count stored events (`multiqueue_size`).
///
/// # Safety
/// `queue` must be valid.
#[must_use]
pub unsafe fn multiqueue_size(queue: *const MultiQueue) -> usize {
    assert!(!queue.is_null());
    unsafe { (*queue).size }
}

struct MulticastEvent {
    event: Event,
    fired: bool,
    refcount: i32,
}

unsafe fn multiqueue_oneshot_event(
    argv: *mut *mut std::ffi::c_void,
) {
    let data = unsafe { *argv }.cast::<MulticastEvent>();
    let event = {
        let data = unsafe { &mut *data };
        if data.fired {
            None
        } else {
            data.fired = true;
            Some(data.event)
        }
    };
    if let Some(mut event) = event
        && let Some(handler) = event.handler
    {
        unsafe { handler(event.argv.as_mut_ptr()) };
    }
    let should_free = {
        let data = unsafe { &mut *data };
        data.refcount -= 1;
        data.refcount == 0
    };
    if should_free {
        unsafe { drop(Box::from_raw(data)) };
    }
}

/// Create an event that may be queued `count` times but fires once
/// (`event_create_oneshot`).
#[must_use]
pub fn event_create_oneshot(event: Event, count: i32) -> Event {
    assert!(count > 0);
    let data = Box::into_raw(Box::new(MulticastEvent {
        event,
        fired: false,
        refcount: count,
    }));
    crate::event::defs::event_create(
        multiqueue_oneshot_event,
        &[data.cast()],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::defs::event_create;

    unsafe fn capture(argv: *mut *mut std::ffi::c_void) {
        let output = unsafe { *argv }.cast::<Vec<usize>>();
        let value = unsafe { *argv.add(1) }.cast::<usize>();
        unsafe { (*output).push(*value) };
    }

    unsafe fn count_put(
        _queue: *mut MultiQueue,
        data: *mut std::ffi::c_void,
    ) {
        unsafe { *data.cast::<usize>() += 1 };
    }

    fn event(output: *mut Vec<usize>, value: *mut usize) -> Event {
        event_create(
            capture,
            &[
                output.cast(),
                value.cast(),
            ],
        )
    }

    #[test]
    fn internal_constructor_initializes_every_queue_field() {
        let mut callback_count = 0usize;
        let data = std::ptr::addr_of_mut!(callback_count).cast();
        let queue = _multiqueue_new(std::ptr::null_mut(), Some(count_put), data);

        unsafe {
            assert!((*queue).parent.is_null());
            assert!((*queue).items.is_empty());
            assert_eq!((*queue).data, data);
            assert_eq!((*queue).size, 0);
            assert_eq!((*queue).next_link_id, 0);
            (*queue).on_put.unwrap()(queue, data);
            assert_eq!(callback_count, 1);
            multiqueue_free(queue);
        }
    }

    #[test]
    fn internal_remove_returns_the_event_and_decrements_size() {
        let queue = multiqueue_new(None, std::ptr::null_mut());
        unsafe {
            multiqueue_put_event(queue, Event::default());
            assert_eq!(multiqueue_size(queue), 1);

            let event = multiqueue_remove(&mut *queue);

            assert!(event.handler.is_none());
            assert!(multiqueue_empty(queue));
            assert_eq!(multiqueue_size(queue), 0);
            multiqueue_free(queue);
        }
    }

    #[test]
    fn root_queue_preserves_fifo_order_and_size() {
        let queue = multiqueue_new(None, std::ptr::null_mut());
        let mut output = Vec::new();
        let output_ptr = std::ptr::addr_of_mut!(output);
        let mut values = [1usize, 2];
        let values_ptr = values.as_mut_ptr();
        unsafe {
            multiqueue_put_event(queue, event(output_ptr, values_ptr));
            multiqueue_put_event(
                queue,
                event(output_ptr, values_ptr.add(1)),
            );
            assert_eq!(multiqueue_size(queue), 2);
            multiqueue_process_events(queue);
            assert_eq!(multiqueue_size(queue), 0);
            multiqueue_free(queue);
        }
        assert_eq!(output, vec![1, 2]);
    }

    #[test]
    fn parent_and_children_share_global_insertion_order() {
        let parent = multiqueue_new(None, std::ptr::null_mut());
        let child1 = unsafe { multiqueue_new_child(parent) };
        let child2 = unsafe { multiqueue_new_child(parent) };
        let mut output = Vec::new();
        let output_ptr = std::ptr::addr_of_mut!(output);
        let mut values = [11usize, 12, 21, 13];
        let values_ptr = values.as_mut_ptr();
        unsafe {
            multiqueue_put_event(child1, event(output_ptr, values_ptr));
            multiqueue_put_event(
                child1,
                event(output_ptr, values_ptr.add(1)),
            );
            multiqueue_put_event(
                child2,
                event(output_ptr, values_ptr.add(2)),
            );
            multiqueue_put_event(
                child1,
                event(output_ptr, values_ptr.add(3)),
            );
            while !multiqueue_empty(parent) {
                let mut event = multiqueue_get(parent);
                event.handler.unwrap()(event.argv.as_mut_ptr());
            }
            multiqueue_free(child1);
            multiqueue_free(child2);
            multiqueue_free(parent);
        }
        assert_eq!(output, vec![11, 12, 21, 13]);
    }

    #[test]
    fn removing_from_child_removes_corresponding_parent_link() {
        let parent = multiqueue_new(None, std::ptr::null_mut());
        let child1 = unsafe { multiqueue_new_child(parent) };
        let child2 = unsafe { multiqueue_new_child(parent) };
        let mut output = Vec::new();
        let output_ptr = std::ptr::addr_of_mut!(output);
        let mut values = [11usize, 21, 12];
        let values_ptr = values.as_mut_ptr();
        unsafe {
            multiqueue_put_event(child1, event(output_ptr, values_ptr));
            multiqueue_put_event(
                child2,
                event(output_ptr, values_ptr.add(1)),
            );
            multiqueue_put_event(
                child1,
                event(output_ptr, values_ptr.add(2)),
            );

            let mut first = multiqueue_get(child1);
            first.handler.unwrap()(first.argv.as_mut_ptr());
            multiqueue_process_events(parent);

            multiqueue_free(child1);
            multiqueue_free(child2);
            multiqueue_free(parent);
        }
        assert_eq!(output, vec![11, 21, 12]);
    }

    #[test]
    fn purge_and_move_preserve_callback_suppression_and_order() {
        let source = multiqueue_new(None, std::ptr::null_mut());
        let destination = multiqueue_new(None, std::ptr::null_mut());
        let mut output = Vec::new();
        let output_ptr = std::ptr::addr_of_mut!(output);
        let mut values = [1usize, 2, 3];
        let values_ptr = values.as_mut_ptr();
        unsafe {
            multiqueue_put_event(source, event(output_ptr, values_ptr));
            multiqueue_put_event(
                source,
                event(output_ptr, values_ptr.add(1)),
            );
            multiqueue_move_events(destination, source);
            assert!(multiqueue_empty(source));
            multiqueue_process_events(destination);

            multiqueue_put_event(
                source,
                event(output_ptr, values_ptr.add(2)),
            );
            multiqueue_purge_events(source);
            multiqueue_free(source);
            multiqueue_free(destination);
        }
        assert_eq!(output, vec![1, 2]);
    }

    #[test]
    fn child_put_notifies_the_parent_callback() {
        let mut count = 0usize;
        let count_ptr = std::ptr::addr_of_mut!(count);
        let parent = multiqueue_new(Some(count_put), count_ptr.cast());
        let child = unsafe { multiqueue_new_child(parent) };
        unsafe {
            multiqueue_put_event(child, Event::default());
            multiqueue_put_event(child, Event::default());
            assert_eq!(*count_ptr, 2);
            multiqueue_purge_events(child);
            multiqueue_free(child);
            multiqueue_free(parent);
        }
    }

    #[test]
    fn freeing_child_removes_its_pending_parent_links() {
        let parent = multiqueue_new(None, std::ptr::null_mut());
        let child = unsafe { multiqueue_new_child(parent) };
        unsafe {
            multiqueue_put_event(child, Event::default());
            assert!(!multiqueue_empty(parent));
            multiqueue_free(child);
            assert!(multiqueue_empty(parent));
            multiqueue_free(parent);
        }
    }

    #[test]
    fn replace_parent_links_future_events_and_empty_get_is_nil() {
        let parent = multiqueue_new(None, std::ptr::null_mut());
        let queue = multiqueue_new(None, std::ptr::null_mut());
        unsafe {
            let nil = multiqueue_get(queue);
            assert!(nil.handler.is_none());
            multiqueue_replace_parent(queue, parent);
            multiqueue_put_event(queue, Event::default());
            assert!(!multiqueue_empty(parent));
            let event = multiqueue_get(parent);
            assert!(event.handler.is_none());
            multiqueue_free(queue);
            multiqueue_free(parent);
        }
    }

    #[test]
    fn oneshot_event_fires_once_across_multiple_queues() {
        let queue1 = multiqueue_new(None, std::ptr::null_mut());
        let queue2 = multiqueue_new(None, std::ptr::null_mut());
        let queue3 = multiqueue_new(None, std::ptr::null_mut());
        let mut output = Vec::new();
        let output_ptr = std::ptr::addr_of_mut!(output);
        let mut value = 42usize;
        let value_ptr = std::ptr::addr_of_mut!(value);
        let oneshot = event_create_oneshot(
            event(output_ptr, value_ptr),
            3,
        );

        unsafe {
            multiqueue_put_event(queue1, oneshot);
            multiqueue_put_event(queue2, oneshot);
            multiqueue_put_event(queue3, oneshot);
            multiqueue_process_events(queue2);
            multiqueue_process_events(queue1);
            multiqueue_process_events(queue3);
            multiqueue_free(queue1);
            multiqueue_free(queue2);
            multiqueue_free(queue3);
        }
        assert_eq!(output, vec![42]);
    }

    #[test]
    fn oneshot_nil_event_still_cleans_up_after_all_copies() {
        let queue1 = multiqueue_new(None, std::ptr::null_mut());
        let queue2 = multiqueue_new(None, std::ptr::null_mut());
        let oneshot = event_create_oneshot(Event::default(), 2);
        unsafe {
            multiqueue_put_event(queue1, oneshot);
            multiqueue_put_event(queue2, oneshot);
            multiqueue_process_events(queue1);
            multiqueue_process_events(queue2);
            multiqueue_free(queue1);
            multiqueue_free(queue2);
        }
    }
}

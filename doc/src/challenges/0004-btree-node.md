# Challenge 4: Towards the memory safety of BTreeMap

- **Status:** Open
- **Tracking Issue:** [Link to issue](https://github.com/model-checking/verify-rust-std/issues/25)
- **Start date:** *2024-07-01*
- **End date:** *2024-12-10*

-------------------


## Goal

Verify the memory safety of the [`alloc::collections::btree::node` module](https://github.com/rust-lang/rust/blob/c290e9de32e8ba6a673ef125fde40eadd395d170/library/alloc/src/collections/btree/node.rs).
This is one of the main modules used for implementing the `BTreeMap` collection, and it includes a lot of unsafe code.

### Success Criteria

The memory safety of all public functions (especially safe ones) containing unsafe code must be verified, e.g.:

1. `LeafNode::new`
1. `NodeRef::new_internal`
1. `NodeRef::as_internal_mut`
1. `NodeRef::len`
1. `NodeRef::ascend`
1. `NodeRef::first_edge`
1. `NodeRef::last_edge`
1. `NodeRef::first_kv`
1. `NodeRef::last_kv`
1. `NodeRef::into_leaf`
1. `NodeRef::keys`
1. `NodeRef::as_leaf_mut`
1. `NodeRef::into_leaf_mut`
1. `NodeRef::as_leaf_dying`
1. `NodeRef::pop_internal_level`
1. `NodeRef::push`
1. `Handle::left_edge`
1. `Handle::right_edge`
1. `Handle::left_kv`
1. `Handle::right_kv`
1. `Handle::insert_recursing`
1. `Handle::descend`
1. `Handle::into_kv`
1. `Handle::key_mut`
1. `Handle::into_val_mut`
1. `Handle::into_kv_mut`
1. `Handle::into_kv_valmut`
1. `Handle::kv_mut`
1. `BalancingContext::do_merge`
1. `BalancingContext::merge_tracking_child_edge`
1. `BalancingContext::steal_left`
1. `BalancingContext::steal_right`
1. `BalancingContext::bulk_steal_left`
1. `BalancingContext::bulk_steal_right`

The verification must be unbounded for functions that are recursive or that contain loops (e.g. `Handle::insert_recursing`).

### List of UBs

All proofs must automatically ensure the absence of the following [undefined behaviors](https://github.com/rust-lang/reference/blob/142b2ed77d33f37a9973772bd95e6144ed9dce43/src/behavior-considered-undefined.md):

* Accessing (loading from or storing to) a place that is dangling or based on a misaligned pointer.
* Reading from uninitialized memory except for padding or unions.
* Mutating immutable bytes.
* Producing an invalid value

Note: All solutions to verification challenges need to satisfy the criteria established in the [challenge book](../general-rules.md)
in addition to the ones listed above.

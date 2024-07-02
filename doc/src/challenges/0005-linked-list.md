# Challenge 5: Verify functions iterating over inductive data type: `linked_list`

- **Status:** Open
- **Tracking Issue:** [Link to issue](https://github.com/model-checking/verify-rust-std/issues/29)
- **Start date:** *24/07/01*
- **End date:** *24/12/10*

-------------------


## Goal

Verify the memory safety of [`alloc::collections::linked_list` functions](https://github.com/rust-lang/rust/blob/c290e9de32e8ba6a673ef125fde40eadd395d170/library/alloc/src/collections/linked_list.rs) that iterate the its internal inductive-defined data type.

### Details

The internal representations of `linked_list` are bi-direction linked list nodes. To unboundedly prove the memory safety of functions that iterating over such inductive-defined data type, we need to illustrate the memory safety for linked lists of arbitrary shape. On the other hand, if we can only prove the memory safety for certain shapes of linked lists, how should we specify the precondition---the assumptions on the shape of the inductive-defined data type---of such functions.  


### Success Criteria

The memory safety of all public functions that iterating over the internal inductive data type must be verified, e.g.:

| Function | Location |
|---------|---------|
|clearn | alloc::collections::linked_list |
|contains| alloc::collections::linked_list |
|split_off| alloc::collections::linked_list |
|remove| alloc::collections::linked_list |
|retain| alloc::collections::linked_list |
|retain_mut| alloc::collections::linked_list |
|extract_if| alloc::collections::linked_list |


The verification must be unbounded---it must hold for linked lists of arbitrary shape.

### List of UBs

All proofs must automatically ensure the absence of the following undefined behaviors [ref](https://github.com/rust-lang/reference/blob/142b2ed77d33f37a9973772bd95e6144ed9dce43/src/behavior-considered-undefined.md):

* Accessing (loading from or storing to) a place that is dangling or based on a misaligned pointer.
* Reading from uninitialized memory except for padding or unions.
* Mutating immutable bytes.
* Producing an invalid value


Note: All solutions to verification challenges need to satisfy the criteria established in the [challenge book](../general-rules.md)
in addition to the ones listed above.

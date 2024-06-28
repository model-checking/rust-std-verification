# Challenge 3: Verifying Raw Pointer Arithmetic Operations

- **Status:** Open
- **Solution:**
- **Tracking Issue:** <https://github.com/model-checking/verify-rust-std/issues/21>
- **Start date:** 24/06/24
- **End date:** 24/12/10

-------------------


## Goal

The goal of this challenge is to verify safety of code that relies on raw pointer arithmetic, and eventual
raw pointer access.

## Motivation

Raw pointer arithmetic is a common operation employed in the implementation of highly optimized code,
as well as containers with dynamic size.
Examples of the former are `str::repeat`, `[u8]::is_ascii`,
while for the latter we have `Vec`, `VecDeque`, `HashMap`.

These unsafe operations are usually abstracted from the end user with the usage of
[safe abstractions](https://doc.rust-lang.org/beta/book/ch19-01-unsafe-rust.html#creating-a-safe-abstraction-over-unsafe-code).
However, bugs in these abstractions may compromise entire applications, potentially becoming a security concern.
See [CVE-2018-1000810](https://www.cvedetails.com/cve/CVE-2018-1000810/) for an example of an issue with an
optimization of `str::repeat`.

These safe abstractions are great candidates for software verification.
They are critical for Rust applications safety, and they are modular by design.

## Description

Rust provides different options for pointer arithmetic, which have different semantics when it comes to safety.
For example, methods such as [`ptr::offset`](https://doc.rust-lang.org/std/primitive.pointer.html#method.offset),
[`ptr::add`](https://doc.rust-lang.org/std/primitive.pointer.html#method.add),
and [`ptr::sub`](https://doc.rust-lang.org/std/primitive.pointer.html#method.sub)
are unsafe, and one of their safety conditions is that:
> - Both the starting and resulting pointer must be either in bounds or one byte past the end of the same allocated object.

I.e., violating this safety condition triggers immediate UB.

On the other hand, wrapping operations such as
[`ptr::wrapping_offset`](https://doc.rust-lang.org/std/primitive.pointer.html#method.wrapping_offset),
[`ptr::wrapping_add`](https://doc.rust-lang.org/std/primitive.pointer.html#method.wrapping_add),
[`ptr::wrapping_sub`](https://doc.rust-lang.org/std/primitive.pointer.html#method.wrapping_sub),
are safe, however, the resulting pointer must not be used to read or write other allocated objects.

Thus, we would like to be able to verify usage of these different methods within the standard library
to ensure they are safely employed,
as well as provide function contracts that can be used by other Rust crates to verify their own usage of these methods.

### Assumptions

For this challenge, we do not require a full proof that is independent of the pointee type `T`.
Instead, we require that the verification is done for the following pointee types:
1. All integer types.
2. At least one `dyn Trait`.
3. At least one slice.
4. For unit type.
5. At least one composite type with multiple non-ZST fields.

### Success Criteria

All the following unsafe functions must be annotated with safety contracts and the contracts have been verified:

| Function                    | Location |
|-----------------------------|----------|
| *const T::add              | core::ptr       |
| *const T::sub              | core::ptr       |
| *const T::offset           | core::ptr       |
| *const T::offset_from      | core::ptr       |
| *const T::byte_add         | core::ptr       |
| *const T::byte_sub         | core::ptr       |
| *const T::byte_offset      | core::ptr       |
| *const T::byte_offset_from | core::ptr       |
| *mut T::add              | core::ptr       |
| *mut T::sub              | core::ptr       |
| *mut T::offset           | core::ptr       |
| *mut T::offset_from      | core::ptr       |
| *mut T::byte_add         | core::ptr       |
| *mut T::byte_sub         | core::ptr       |
| *mut T::byte_offset      | core::ptr       |
| *mut T::byte_offset_from | core::ptr       |

At least 3 of the following usages were proven safe:

| Function          | Location      |
|-------------------|---------------|
| \[u8\]::is_asc_ii | core::slice   |
| String::remove    | alloc::string |
 | Vec::swap_remove | alloc::vec |
 | Option::as_slice | core::option |
 | VecDeque::swap   | collections::vec_deque |

All proofs must automatically ensure the absence of the following undefined behaviors [ref](https://github.com/rust-lang/reference/blob/142b2ed77d33f37a9973772bd95e6144ed9dce43/src/behavior-considered-undefined.md):

- Accessing (loading from or storing to) a place that is dangling or based on a misaligned pointer.
- Performing a place projection that violates the requirements of in-bounds pointer arithmetic.
A place projection is a field expression, a tuple index expression, or an array/slice index expression.
- Invoking undefined behavior via compiler intrinsics.
- Producing an invalid value, even in private fields and locals.

Note: All solutions to verification challenges need to satisfy the criteria established in the [challenge book](../general-rules.md)
in addition to the ones listed above.


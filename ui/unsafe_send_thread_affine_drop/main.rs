// Test cases for `unsafe_send_thread_affine_drop`.
#![allow(
    dead_code,
    unused,
    unsafe_code,
    unknown_lints,
    topological_ordering,
    clippy::non_send_fields_in_send_ty
)]

use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::rc::Rc;

// The contract is declared beside the type rather than in workspace config.
#[rust_lints_contracts::thread_affine_drop(reason = "must be released on its creating thread")]
struct ThreadAffineResource {
    _not_send: PhantomData<Rc<()>>,
}

// Directly contradicts the declared contract.
unsafe impl Send for ThreadAffineResource {}
//~^ WARNING: permits thread-affine state to be dropped on another thread

// Direct ownership is subject to automatic field drop glue.
struct Direct {
    resource: ThreadAffineResource,
}
unsafe impl Send for Direct {}
//~^ WARNING: permits thread-affine state to be dropped on another thread

// An outer Drop impl does not suppress field drop glue after Drop::drop returns.
struct EmptyDrop {
    resource: ThreadAffineResource,
}
unsafe impl Send for EmptyDrop {}
//~^ WARNING: permits thread-affine state to be dropped on another thread
impl Drop for EmptyDrop {
    fn drop(&mut self) {}
}

// Ordinary owning wrappers retain the destruction hazard.
struct Nested {
    resource: Option<Box<ThreadAffineResource>>,
}
unsafe impl Send for Nested {}
//~^ WARNING: permits thread-affine state to be dropped on another thread

struct Collection {
    resources: Vec<ThreadAffineResource>,
}
unsafe impl Send for Collection {}
//~^ WARNING: permits thread-affine state to be dropped on another thread

// Dependency-owned contracts remain configurable when source annotation is
// impossible. The test config gives String a synthetic contract for coverage.
struct ExternalConfigured {
    resource: std::path::PathBuf,
}
unsafe impl Send for ExternalConfigured {}
//~^ WARNING: permits thread-affine state to be dropped on another thread

// ManuallyDrop is the structural proof that automatic field drop glue cannot
// destroy the resource. Explicit cleanup remains the unsafe impl author's job.
struct ExplicitDestruction {
    resource: ManuallyDrop<ThreadAffineResource>,
}
unsafe impl Send for ExplicitDestruction {}
impl Drop for ExplicitDestruction {
    fn drop(&mut self) {
        // Dispatch destruction to the creating thread in real code.
    }
}

// Borrowed and marker-only occurrences do not own a value to destroy.
struct Borrowed<'a> {
    resource: &'a ThreadAffineResource,
}
unsafe impl<'a> Send for Borrowed<'a> {}

struct MarkerOnly {
    resource: PhantomData<ThreadAffineResource>,
}
unsafe impl Send for MarkerOnly {}

// `!Send` alone says nothing about destruction affinity and must remain silent.
struct UnrelatedNonSend {
    value: Rc<String>,
}
unsafe impl Send for UnrelatedNonSend {}

struct Generic<T> {
    value: T,
}
unsafe impl<T> Send for Generic<T> {}

fn main() {}

// Expanding instantiations never repeat and must terminate without a warning.
struct Node<T> {
    value: T,
    next: Option<Box<Node<Vec<T>>>>,
}
unsafe impl<T> Send for Node<T> {}

// An expanding branch must not hide a hazardous sibling within the same field.
struct RecursiveSibling<T> {
    next: Option<Box<Node<Vec<T>>>>,
    resource: ThreadAffineResource,
}
struct RecursiveOwner<T> {
    inner: RecursiveSibling<T>,
}
unsafe impl<T> Send for RecursiveOwner<T> {}
//~^ WARNING: permits thread-affine state to be dropped on another thread

// Repeated definitions with different arguments are not cycles.
struct RepeatedWrapper {
    inner: Generic<Generic<ThreadAffineResource>>,
}
unsafe impl Send for RepeatedWrapper {}
//~^ WARNING: permits thread-affine state to be dropped on another thread

// An exhausted field budget must not suppress diagnostics on another field.
struct SeparateSibling<T> {
    next: Node<T>,
    resource: ThreadAffineResource,
}
unsafe impl<T> Send for SeparateSibling<T> {}
//~^ WARNING: permits thread-affine state to be dropped on another thread

// Branching expansion exercises the work budget before the depth bound.
struct Branching<T> {
    left: Option<Box<Branching<Vec<T>>>>,
    right: Option<Box<Branching<Option<T>>>>,
    value: T,
}
unsafe impl<T> Send for Branching<T> {}

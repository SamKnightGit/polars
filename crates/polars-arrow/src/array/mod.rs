//! Contains the [`Array`] and [`MutableArray`] trait objects declaring arrays,
//! as well as concrete arrays (such as [`Utf8Array`] and [`MutableUtf8Array`]).
//!
//! Fixed-length containers with optional values
//! that are laid in memory according to the Arrow specification.
//! Each array type has its own `struct`. The following are the main array types:
//! * [`PrimitiveArray`] and [`MutablePrimitiveArray`], an array of values with a fixed length such as integers, floats, etc.
//! * [`BooleanArray`] and [`MutableBooleanArray`], an array of boolean values (stored as a bitmap)
//! * [`Utf8Array`] and [`MutableUtf8Array`], an array of variable length utf8 values
//! * [`BinaryArray`] and [`MutableBinaryArray`], an array of opaque variable length values
//! * [`ListArray`] and [`MutableListArray`], an array of arrays (e.g. `[[1, 2], None, [], [None]]`)
//! * [`StructArray`] and [`MutableStructArray`], an array of arrays identified by a string (e.g. `{"a": [1, 2], "b": [true, false]}`)
//!
//! All immutable arrays implement the trait object [`Array`] and that can be downcast
//! to a concrete struct based on [`PhysicalType`](crate::datatypes::PhysicalType) available from [`Array::dtype`].
//! All immutable arrays are backed by [`Buffer`](crate::buffer::Buffer) and thus cloning and slicing them is `O(1)`.
//!
//! Most arrays contain a [`MutableArray`] counterpart that is neither cloneable nor sliceable, but
//! can be operated in-place.
#![allow(unsafe_op_in_unsafe_fn)]
use std::any::Any;
use std::sync::Arc;

use crate::bitmap::{Bitmap, MutableBitmap};
use crate::datatypes::ArrowDataType;

pub mod physical_binary;
#[cfg(feature = "proptest")]
pub mod proptest;

pub trait Splitable: Sized {
    fn check_bound(&self, offset: usize) -> bool;

    /// Split [`Self`] at `offset` where `offset <= self.len()`.
    #[inline]
    #[must_use]
    fn split_at(&self, offset: usize) -> (Self, Self) {
        assert!(self.check_bound(offset));
        unsafe { self._split_at_unchecked(offset) }
    }

    /// Split [`Self`] at `offset` without checking `offset <= self.len()`.
    ///
    /// # Safety
    ///
    /// Safe if `offset <= self.len()`.
    #[inline]
    #[must_use]
    unsafe fn split_at_unchecked(&self, offset: usize) -> (Self, Self) {
        debug_assert!(self.check_bound(offset));
        unsafe { self._split_at_unchecked(offset) }
    }

    /// Internal implementation of `split_at_unchecked`. For any usage, prefer the using
    /// `split_at` or `split_at_unchecked`.
    ///
    /// # Safety
    ///
    /// Safe if `offset <= self.len()`.
    unsafe fn _split_at_unchecked(&self, offset: usize) -> (Self, Self);
}

/// A trait representing an immutable Arrow array. Arrow arrays are trait objects
/// that are infallibly downcast to concrete types according to the [`Array::dtype`].
pub trait Array: Send + Sync + dyn_clone::DynClone + 'static {
    /// Converts itself to a reference of [`Any`], which enables downcasting to concrete types.
    fn as_any(&self) -> &dyn Any;

    /// Converts itself to a mutable reference of [`Any`], which enables mutable downcasting to concrete types.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// The length of the [`Array`]. Every array has a length corresponding to the number of
    /// elements (slots).
    fn len(&self) -> usize;

    /// whether the array is empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The [`ArrowDataType`] of the [`Array`]. In combination with [`Array::as_any`], this can be
    /// used to downcast trait objects (`dyn Array`) to concrete arrays.
    fn dtype(&self) -> &ArrowDataType;

    /// The validity of the [`Array`]: every array has an optional [`Bitmap`] that, when available
    /// specifies whether the array slot is valid or not (null).
    /// When the validity is [`None`], all slots are valid.
    fn validity(&self) -> Option<&Bitmap>;

    /// The number of null slots on this [`Array`].
    /// # Implementation
    /// This is `O(1)` since the number of null elements is pre-computed.
    #[inline]
    fn null_count(&self) -> usize {
        if self.dtype() == &ArrowDataType::Null {
            return self.len();
        };
        self.validity()
            .as_ref()
            .map(|x| x.unset_bits())
            .unwrap_or(0)
    }

    #[inline]
    fn has_nulls(&self) -> bool {
        self.null_count() > 0
    }

    /// Returns whether slot `i` is null.
    /// # Panic
    /// Panics iff `i >= self.len()`.
    #[inline]
    fn is_null(&self, i: usize) -> bool {
        assert!(i < self.len());
        unsafe { self.is_null_unchecked(i) }
    }

    /// Returns whether slot `i` is null.
    ///
    /// # Safety
    /// The caller must ensure `i < self.len()`
    #[inline]
    unsafe fn is_null_unchecked(&self, i: usize) -> bool {
        self.validity()
            .as_ref()
            .map(|x| !x.get_bit_unchecked(i))
            .unwrap_or(false)
    }

    /// Returns whether slot `i` is valid.
    /// # Panic
    /// Panics iff `i >= self.len()`.
    #[inline]
    fn is_valid(&self, i: usize) -> bool {
        !self.is_null(i)
    }

    /// Split [`Self`] at `offset` into two boxed [`Array`]s where `offset <= self.len()`.
    #[must_use]
    fn split_at_boxed(&self, offset: usize) -> (Box<dyn Array>, Box<dyn Array>);

    /// Split [`Self`] at `offset` into two boxed [`Array`]s without checking `offset <= self.len()`.
    ///
    /// # Safety
    ///
    /// Safe if `offset <= self.len()`.
    #[must_use]
    unsafe fn split_at_boxed_unchecked(&self, offset: usize) -> (Box<dyn Array>, Box<dyn Array>);

    /// Slices this [`Array`].
    /// # Implementation
    /// This operation is `O(1)` over `len`.
    /// # Panic
    /// This function panics iff `offset + length > self.len()`.
    fn slice(&mut self, offset: usize, length: usize);

    /// Slices the [`Array`].
    /// # Implementation
    /// This operation is `O(1)`.
    ///
    /// # Safety
    /// The caller must ensure that `offset + length <= self.len()`
    unsafe fn slice_unchecked(&mut self, offset: usize, length: usize);

    /// Returns a slice of this [`Array`].
    /// # Implementation
    /// This operation is `O(1)` over `len`.
    /// # Panic
    /// This function panics iff `offset + length > self.len()`.
    #[must_use]
    fn sliced(&self, offset: usize, length: usize) -> Box<dyn Array> {
        if length == 0 {
            return new_empty_array(self.dtype().clone());
        }
        let mut new = self.to_boxed();
        new.slice(offset, length);
        new
    }

    /// Returns a slice of this [`Array`].
    /// # Implementation
    /// This operation is `O(1)` over `len`, as it amounts to increase two ref counts
    /// and moving the struct to the heap.
    ///
    /// # Safety
    /// The caller must ensure that `offset + length <= self.len()`
    #[must_use]
    unsafe fn sliced_unchecked(&self, offset: usize, length: usize) -> Box<dyn Array> {
        debug_assert!(offset + length <= self.len());
        let mut new = self.to_boxed();
        new.slice_unchecked(offset, length);
        new
    }

    /// Clones this [`Array`] with a new assigned bitmap.
    /// # Panic
    /// This function panics iff `validity.len() != self.len()`.
    fn with_validity(&self, validity: Option<Bitmap>) -> Box<dyn Array>;

    /// Clone a `&dyn Array` to an owned `Box<dyn Array>`.
    fn to_boxed(&self) -> Box<dyn Array>;
}

dyn_clone::clone_trait_object!(Array);

pub trait IntoBoxedArray {
    fn into_boxed(self) -> Box<dyn Array>;
}

impl<A: Array> IntoBoxedArray for A {
    #[inline(always)]
    fn into_boxed(self) -> Box<dyn Array> {
        Box::new(self) as _
    }
}
impl IntoBoxedArray for Box<dyn Array> {
    #[inline(always)]
    fn into_boxed(self) -> Box<dyn Array> {
        self
    }
}

/// A trait describing a mutable array; i.e. an array whose values can be changed.
///
/// Mutable arrays cannot be cloned but can be mutated in place,
/// thereby making them useful to perform numeric operations without allocations.
/// As in [`Array`], concrete arrays (such as [`MutablePrimitiveArray`]) implement how they are mutated.
pub trait MutableArray: std::fmt::Debug + Send + Sync {
    /// The [`ArrowDataType`] of the array.
    fn dtype(&self) -> &ArrowDataType;

    /// The length of the array.
    fn len(&self) -> usize;

    /// Whether the array is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The optional validity of the array.
    fn validity(&self) -> Option<&MutableBitmap>;

    /// Convert itself to an (immutable) [`Array`].
    fn as_box(&mut self) -> Box<dyn Array>;

    /// Convert itself to an (immutable) atomically reference counted [`Array`].
    // This provided implementation has an extra allocation as it first
    // boxes `self`, then converts the box into an `Arc`. Implementors may wish
    // to avoid an allocation by skipping the box completely.
    fn as_arc(&mut self) -> std::sync::Arc<dyn Array> {
        self.as_box().into()
    }

    /// Convert to `Any`, to enable dynamic casting.
    fn as_any(&self) -> &dyn Any;

    /// Convert to mutable `Any`, to enable dynamic casting.
    fn as_mut_any(&mut self) -> &mut dyn Any;

    /// Adds a new null element to the array.
    fn push_null(&mut self);

    /// Whether `index` is valid / set.
    /// # Panic
    /// Panics if `index >= self.len()`.
    #[inline]
    fn is_valid(&self, index: usize) -> bool {
        self.validity()
            .as_ref()
            .map(|x| x.get(index))
            .unwrap_or(true)
    }

    /// Reserves additional slots to its capacity.
    fn reserve(&mut self, additional: usize);

    /// Shrink the array to fit its length.
    fn shrink_to_fit(&mut self);
}

impl MutableArray for Box<dyn MutableArray> {
    fn len(&self) -> usize {
        self.as_ref().len()
    }

    fn validity(&self) -> Option<&MutableBitmap> {
        self.as_ref().validity()
    }

    fn as_box(&mut self) -> Box<dyn Array> {
        self.as_mut().as_box()
    }

    fn as_arc(&mut self) -> Arc<dyn Array> {
        self.as_mut().as_arc()
    }

    fn dtype(&self) -> &ArrowDataType {
        self.as_ref().dtype()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self.as_ref().as_any()
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self.as_mut().as_mut_any()
    }

    #[inline]
    fn push_null(&mut self) {
        self.as_mut().push_null()
    }

    fn shrink_to_fit(&mut self) {
        self.as_mut().shrink_to_fit();
    }

    fn reserve(&mut self, additional: usize) {
        self.as_mut().reserve(additional);
    }
}

macro_rules! general_dyn {
    ($array:expr, $ty:ty, $f:expr) => {{
        let array = $array.as_any().downcast_ref::<$ty>().unwrap();
        ($f)(array)
    }};
}

macro_rules! fmt_dyn {
    ($array:expr, $ty:ty, $f:expr) => {{
        let mut f = |x: &$ty| x.fmt($f);
        general_dyn!($array, $ty, f)
    }};
}

impl std::fmt::Debug for dyn Array + '_ {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use crate::datatypes::PhysicalType::*;
        match self.dtype().to_physical_type() {
            Null => fmt_dyn!(self, NullArray, f),
            Boolean => fmt_dyn!(self, BooleanArray, f),
            Primitive(primitive) => with_match_primitive_type_full!(primitive, |$T| {
                fmt_dyn!(self, PrimitiveArray<$T>, f)
            }),
            BinaryView => fmt_dyn!(self, BinaryViewArray, f),
            Utf8View => fmt_dyn!(self, Utf8ViewArray, f),
            Binary => fmt_dyn!(self, BinaryArray<i32>, f),
            LargeBinary => fmt_dyn!(self, BinaryArray<i64>, f),
            FixedSizeBinary => fmt_dyn!(self, FixedSizeBinaryArray, f),
            Utf8 => fmt_dyn!(self, Utf8Array::<i32>, f),
            LargeUtf8 => fmt_dyn!(self, Utf8Array::<i64>, f),
            List => fmt_dyn!(self, ListArray::<i32>, f),
            LargeList => fmt_dyn!(self, ListArray::<i64>, f),
            FixedSizeList => fmt_dyn!(self, FixedSizeListArray, f),
            Struct => fmt_dyn!(self, StructArray, f),
            Union => fmt_dyn!(self, UnionArray, f),
            Dictionary(key_type) => {
                match_integer_type!(key_type, |$T| {
                    fmt_dyn!(self, DictionaryArray::<$T>, f)
                })
            },
            Map => fmt_dyn!(self, MapArray, f),
        }
    }
}

/// Creates a new [`Array`] with a [`Array::len`] of 0.
pub fn new_empty_array(dtype: ArrowDataType) -> Box<dyn Array> {
    use crate::datatypes::PhysicalType::*;
    match dtype.to_physical_type() {
        Null => Box::new(NullArray::new_empty(dtype)),
        Boolean => Box::new(BooleanArray::new_empty(dtype)),
        Primitive(primitive) => with_match_primitive_type_full!(primitive, |$T| {
            Box::new(PrimitiveArray::<$T>::new_empty(dtype))
        }),
        Binary => Box::new(BinaryArray::<i32>::new_empty(dtype)),
        LargeBinary => Box::new(BinaryArray::<i64>::new_empty(dtype)),
        FixedSizeBinary => Box::new(FixedSizeBinaryArray::new_empty(dtype)),
        Utf8 => Box::new(Utf8Array::<i32>::new_empty(dtype)),
        LargeUtf8 => Box::new(Utf8Array::<i64>::new_empty(dtype)),
        List => Box::new(ListArray::<i32>::new_empty(dtype)),
        LargeList => Box::new(ListArray::<i64>::new_empty(dtype)),
        FixedSizeList => Box::new(FixedSizeListArray::new_empty(dtype)),
        Struct => Box::new(StructArray::new_empty(dtype)),
        Union => Box::new(UnionArray::new_empty(dtype)),
        Map => Box::new(MapArray::new_empty(dtype)),
        Utf8View => Box::new(Utf8ViewArray::new_empty(dtype)),
        BinaryView => Box::new(BinaryViewArray::new_empty(dtype)),
        Dictionary(key_type) => {
            match_integer_type!(key_type, |$T| {
                Box::new(DictionaryArray::<$T>::new_empty(dtype))
            })
        },
    }
}

/// Creates a new [`Array`] of [`ArrowDataType`] `dtype` and `length`.
///
/// The array is guaranteed to have [`Array::null_count`] equal to [`Array::len`]
/// for all types except Union, which does not have a validity.
pub fn new_null_array(dtype: ArrowDataType, length: usize) -> Box<dyn Array> {
    use crate::datatypes::PhysicalType::*;
    match dtype.to_physical_type() {
        Null => Box::new(NullArray::new_null(dtype, length)),
        Boolean => Box::new(BooleanArray::new_null(dtype, length)),
        Primitive(primitive) => with_match_primitive_type_full!(primitive, |$T| {
            Box::new(PrimitiveArray::<$T>::new_null(dtype, length))
        }),
        Binary => Box::new(BinaryArray::<i32>::new_null(dtype, length)),
        LargeBinary => Box::new(BinaryArray::<i64>::new_null(dtype, length)),
        FixedSizeBinary => Box::new(FixedSizeBinaryArray::new_null(dtype, length)),
        Utf8 => Box::new(Utf8Array::<i32>::new_null(dtype, length)),
        LargeUtf8 => Box::new(Utf8Array::<i64>::new_null(dtype, length)),
        List => Box::new(ListArray::<i32>::new_null(dtype, length)),
        LargeList => Box::new(ListArray::<i64>::new_null(dtype, length)),
        FixedSizeList => Box::new(FixedSizeListArray::new_null(dtype, length)),
        Struct => Box::new(StructArray::new_null(dtype, length)),
        Union => Box::new(UnionArray::new_null(dtype, length)),
        Map => Box::new(MapArray::new_null(dtype, length)),
        BinaryView => Box::new(BinaryViewArray::new_null(dtype, length)),
        Utf8View => Box::new(Utf8ViewArray::new_null(dtype, length)),
        Dictionary(key_type) => {
            match_integer_type!(key_type, |$T| {
                Box::new(DictionaryArray::<$T>::new_null(dtype, length))
            })
        },
    }
}

macro_rules! clone_dyn {
    ($array:expr, $ty:ty) => {{
        let f = |x: &$ty| Box::new(x.clone());
        general_dyn!($array, $ty, f)
    }};
}

// macro implementing `sliced` and `sliced_unchecked`
macro_rules! impl_sliced {
    () => {
        /// Returns this array sliced.
        /// # Implementation
        /// This function is `O(1)`.
        /// # Panics
        /// iff `offset + length > self.len()`.
        #[inline]
        #[must_use]
        pub fn sliced(self, offset: usize, length: usize) -> Self {
            let total = offset
                .checked_add(length)
                .expect("offset + length overflowed");
            assert!(
                total <= self.len(),
                "the offset of the new Buffer cannot exceed the existing length"
            );
            unsafe { Self::sliced_unchecked(self, offset, length) }
        }

        /// Returns this array sliced.
        /// # Implementation
        /// This function is `O(1)`.
        ///
        /// # Safety
        /// The caller must ensure that `offset + length <= self.len()`.
        #[inline]
        #[must_use]
        pub unsafe fn sliced_unchecked(mut self, offset: usize, length: usize) -> Self {
            Self::slice_unchecked(&mut self, offset, length);
            self
        }
    };
}

// macro implementing `with_validity` and `set_validity`
macro_rules! impl_mut_validity {
    () => {
        /// Returns this array with a new validity.
        /// # Panic
        /// Panics iff `validity.len() != self.len()`.
        #[must_use]
        #[inline]
        pub fn with_validity(mut self, validity: Option<Bitmap>) -> Self {
            self.set_validity(validity);
            self
        }

        /// Sets the validity of this array.
        /// # Panics
        /// This function panics iff `values.len() != self.len()`.
        #[inline]
        pub fn set_validity(&mut self, validity: Option<Bitmap>) {
            if matches!(&validity, Some(bitmap) if bitmap.len() != self.len()) {
                panic!("validity must be equal to the array's length")
            }
            self.validity = validity;
        }

        /// Takes the validity of this array, leaving it without a validity mask.
        #[inline]
        pub fn take_validity(&mut self) -> Option<Bitmap> {
            self.validity.take()
        }
    }
}

// macro implementing `with_validity`, `set_validity` and `apply_validity` for mutable arrays
macro_rules! impl_mutable_array_mut_validity {
    () => {
        /// Returns this array with a new validity.
        /// # Panic
        /// Panics iff `validity.len() != self.len()`.
        #[must_use]
        #[inline]
        pub fn with_validity(mut self, validity: Option<MutableBitmap>) -> Self {
            self.set_validity(validity);
            self
        }

        /// Sets the validity of this array.
        /// # Panics
        /// This function panics iff `values.len() != self.len()`.
        #[inline]
        pub fn set_validity(&mut self, validity: Option<MutableBitmap>) {
            if matches!(&validity, Some(bitmap) if bitmap.len() != self.len()) {
                panic!("validity must be equal to the array's length")
            }
            self.validity = validity;
        }

        /// Applies a function `f` to the validity of this array.
        ///
        /// This is an API to leverage clone-on-write
        /// # Panics
        /// This function panics if the function `f` modifies the length of the [`Bitmap`].
        #[inline]
        pub fn apply_validity<F: FnOnce(MutableBitmap) -> MutableBitmap>(&mut self, f: F) {
            if let Some(validity) = std::mem::take(&mut self.validity) {
                self.set_validity(Some(f(validity)))
            }
        }

    }
}

// macro implementing `boxed` and `arced`
macro_rules! impl_into_array {
    () => {
        /// Boxes this array into a [`Box<dyn Array>`].
        pub fn boxed(self) -> Box<dyn Array> {
            Box::new(self)
        }

        /// Arcs this array into a [`std::sync::Arc<dyn Array>`].
        pub fn arced(self) -> std::sync::Arc<dyn Array> {
            std::sync::Arc::new(self)
        }
    };
}

// macro implementing common methods of trait `Array`
macro_rules! impl_common_array {
    () => {
        #[inline]
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        #[inline]
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }

        #[inline]
        fn len(&self) -> usize {
            self.len()
        }

        #[inline]
        fn dtype(&self) -> &ArrowDataType {
            &self.dtype
        }

        #[inline]
        fn split_at_boxed(&self, offset: usize) -> (Box<dyn Array>, Box<dyn Array>) {
            let (lhs, rhs) = $crate::array::Splitable::split_at(self, offset);
            (Box::new(lhs), Box::new(rhs))
        }

        #[inline]
        unsafe fn split_at_boxed_unchecked(
            &self,
            offset: usize,
        ) -> (Box<dyn Array>, Box<dyn Array>) {
            let (lhs, rhs) = unsafe { $crate::array::Splitable::split_at_unchecked(self, offset) };
            (Box::new(lhs), Box::new(rhs))
        }

        #[inline]
        fn slice(&mut self, offset: usize, length: usize) {
            self.slice(offset, length);
        }

        #[inline]
        unsafe fn slice_unchecked(&mut self, offset: usize, length: usize) {
            self.slice_unchecked(offset, length);
        }

        #[inline]
        fn to_boxed(&self) -> Box<dyn Array> {
            Box::new(self.clone())
        }
    };
}

/// Clones a dynamic [`Array`].
/// # Implementation
/// This operation is `O(1)` over `len`, as it amounts to increase two ref counts
/// and moving the concrete struct under a `Box`.
pub fn clone(array: &dyn Array) -> Box<dyn Array> {
    use crate::datatypes::PhysicalType::*;
    match array.dtype().to_physical_type() {
        Null => clone_dyn!(array, NullArray),
        Boolean => clone_dyn!(array, BooleanArray),
        Primitive(primitive) => with_match_primitive_type_full!(primitive, |$T| {
            clone_dyn!(array, PrimitiveArray<$T>)
        }),
        Binary => clone_dyn!(array, BinaryArray<i32>),
        LargeBinary => clone_dyn!(array, BinaryArray<i64>),
        FixedSizeBinary => clone_dyn!(array, FixedSizeBinaryArray),
        Utf8 => clone_dyn!(array, Utf8Array::<i32>),
        LargeUtf8 => clone_dyn!(array, Utf8Array::<i64>),
        List => clone_dyn!(array, ListArray::<i32>),
        LargeList => clone_dyn!(array, ListArray::<i64>),
        FixedSizeList => clone_dyn!(array, FixedSizeListArray),
        Struct => clone_dyn!(array, StructArray),
        Union => clone_dyn!(array, UnionArray),
        Map => clone_dyn!(array, MapArray),
        BinaryView => clone_dyn!(array, BinaryViewArray),
        Utf8View => clone_dyn!(array, Utf8ViewArray),
        Dictionary(key_type) => {
            match_integer_type!(key_type, |$T| {
                clone_dyn!(array, DictionaryArray::<$T>)
            })
        },
    }
}

// see https://users.rust-lang.org/t/generic-for-dyn-a-or-box-dyn-a-or-arc-dyn-a/69430/3
// for details
impl<'a> AsRef<(dyn Array + 'a)> for dyn Array {
    fn as_ref(&self) -> &(dyn Array + 'a) {
        self
    }
}

mod binary;
mod boolean;
pub mod builder;
mod dictionary;
mod fixed_size_binary;
mod fixed_size_list;
mod list;
pub use list::LIST_VALUES_NAME;
mod map;
mod null;
mod primitive;
pub mod specification;
mod static_array;
mod static_array_collect;
mod struct_;
mod total_ord;
mod union;
mod utf8;

mod equal;
mod ffi;
mod fmt;
#[doc(hidden)]
pub mod indexable;
pub mod iterator;

mod binview;
mod values;

pub use binary::{
    BinaryArray, BinaryArrayBuilder, BinaryValueIter, MutableBinaryArray, MutableBinaryValuesArray,
};
pub use binview::{
    BinaryViewArray, BinaryViewArrayBuilder, BinaryViewArrayGeneric, BinaryViewArrayGenericBuilder,
    MutableBinaryViewArray, MutablePlBinary, MutablePlString, Utf8ViewArray, Utf8ViewArrayBuilder,
    View, ViewType,
};
pub use boolean::{BooleanArray, BooleanArrayBuilder, MutableBooleanArray};
pub use dictionary::{DictionaryArray, DictionaryKey, MutableDictionaryArray};
pub use equal::equal;
pub use fixed_size_binary::{
    FixedSizeBinaryArray, FixedSizeBinaryArrayBuilder, MutableFixedSizeBinaryArray,
};
pub use fixed_size_list::{
    FixedSizeListArray, FixedSizeListArrayBuilder, MutableFixedSizeListArray,
};
pub use fmt::{get_display, get_value_display};
pub(crate) use iterator::ArrayAccessor;
pub use iterator::ArrayValuesIter;
pub use list::{ListArray, ListArrayBuilder, ListValuesIter, MutableListArray};
pub use map::MapArray;
pub use null::{MutableNullArray, NullArray, NullArrayBuilder};
use polars_error::PolarsResult;
pub use primitive::*;
pub use static_array::{ParameterFreeDtypeStaticArray, StaticArray};
pub use static_array_collect::{ArrayCollectIterExt, ArrayFromIter, ArrayFromIterDtype};
pub use struct_::{StructArray, StructArrayBuilder};
pub use union::UnionArray;
pub use utf8::{MutableUtf8Array, MutableUtf8ValuesArray, Utf8Array, Utf8ValuesIter};
pub use values::ValueSize;

#[cfg(feature = "proptest")]
pub use self::boolean::proptest::boolean_array;
pub(crate) use self::ffi::{FromFfi, ToFfi, offset_buffers_children_dictionary};
use crate::{match_integer_type, with_match_primitive_type_full};

/// A trait describing the ability of a struct to create itself from a iterator.
/// This is similar to [`Extend`], but accepted the creation to error.
pub trait TryExtend<A> {
    /// Fallible version of [`Extend::extend`].
    fn try_extend<I: IntoIterator<Item = A>>(&mut self, iter: I) -> PolarsResult<()>;
}

/// A trait describing the ability of a struct to receive new items.
pub trait TryPush<A> {
    /// Tries to push a new element.
    fn try_push(&mut self, item: A) -> PolarsResult<()>;
}

/// A trait describing the ability of a struct to receive new items.
pub trait PushUnchecked<A> {
    /// Push a new element that holds the invariants of the struct.
    ///
    /// # Safety
    /// The items must uphold the invariants of the struct
    /// Read the specific implementation of the trait to understand what these are.
    unsafe fn push_unchecked(&mut self, item: A);
}

/// A trait describing the ability of a struct to extend from a reference of itself.
/// Specialization of [`TryExtend`].
pub trait TryExtendFromSelf {
    /// Tries to extend itself with elements from `other`, failing only on overflow.
    fn try_extend_from_self(&mut self, other: &Self) -> PolarsResult<()>;
}

/// Trait that [`BinaryArray`] and [`Utf8Array`] implement for the purposes of DRY.
/// # Safety
/// The implementer must ensure that
/// 1. `offsets.len() > 0`
/// 2. `offsets[i] >= offsets[i-1] for all i`
/// 3. `offsets[i] < values.len() for all i`
pub unsafe trait GenericBinaryArray<O: crate::offset::Offset>: Array {
    /// The values of the array
    fn values(&self) -> &[u8];
    /// The offsets of the array
    fn offsets(&self) -> &[O];
}

pub type ArrayRef = Box<dyn Array>;

impl Splitable for Option<Bitmap> {
    #[inline(always)]
    fn check_bound(&self, offset: usize) -> bool {
        self.as_ref().is_none_or(|v| offset <= v.len())
    }

    unsafe fn _split_at_unchecked(&self, offset: usize) -> (Self, Self) {
        self.as_ref().map_or((None, None), |bm| {
            let (lhs, rhs) = unsafe { bm.split_at_unchecked(offset) };
            (
                (lhs.unset_bits() > 0).then_some(lhs),
                (rhs.unset_bits() > 0).then_some(rhs),
            )
        })
    }
}

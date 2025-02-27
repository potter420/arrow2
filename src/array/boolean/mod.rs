use crate::{
    bitmap::Bitmap,
    datatypes::{DataType, PhysicalType},
};
use either::Either;

use super::Array;

mod ffi;
pub(super) mod fmt;
mod from;
mod iterator;
mod mutable;

pub use iterator::*;
pub use mutable::*;

/// The Arrow's equivalent to an immutable `Vec<Option<bool>>`, but with `1/16` of its size.
/// Cloning and slicing this struct is `O(1)`.
#[derive(Clone)]
pub struct BooleanArray {
    data_type: DataType,
    values: Bitmap,
    validity: Option<Bitmap>,
}

impl BooleanArray {
    /// Returns a new empty [`BooleanArray`].
    pub fn new_empty(data_type: DataType) -> Self {
        Self::from_data(data_type, Bitmap::new(), None)
    }

    /// Returns a new [`BooleanArray`] whose all slots are null / `None`.
    pub fn new_null(data_type: DataType, length: usize) -> Self {
        let bitmap = Bitmap::new_zeroed(length);
        Self::from_data(data_type, bitmap.clone(), Some(bitmap))
    }

    /// The canonical method to create a [`BooleanArray`] out of low-end APIs.
    /// # Panics
    /// This function panics iff:
    /// * The validity is not `None` and its length is different from `values`'s length
    pub fn from_data(data_type: DataType, values: Bitmap, validity: Option<Bitmap>) -> Self {
        if let Some(ref validity) = validity {
            assert_eq!(values.len(), validity.len());
        }
        if data_type.to_physical_type() != PhysicalType::Boolean {
            panic!("BooleanArray can only be initialized with DataType::Boolean")
        }
        Self {
            data_type,
            values,
            validity,
        }
    }

    /// Returns a slice of this [`BooleanArray`].
    /// # Implementation
    /// This operation is `O(1)` as it amounts to increase two ref counts.
    /// # Panic
    /// This function panics iff `offset + length >= self.len()`.
    #[inline]
    pub fn slice(&self, offset: usize, length: usize) -> Self {
        assert!(
            offset + length <= self.len(),
            "the offset of the new Buffer cannot exceed the existing length"
        );
        unsafe { self.slice_unchecked(offset, length) }
    }

    /// Returns a slice of this [`BooleanArray`].
    /// # Implementation
    /// This operation is `O(1)` as it amounts to increase two ref counts.
    /// # Safety
    /// The caller must ensure that `offset + length <= self.len()`.
    #[inline]
    pub unsafe fn slice_unchecked(&self, offset: usize, length: usize) -> Self {
        let validity = self
            .validity
            .clone()
            .map(|x| x.slice_unchecked(offset, length));
        Self {
            data_type: self.data_type.clone(),
            values: self.values.clone().slice_unchecked(offset, length),
            validity,
        }
    }

    /// Sets the validity bitmap on this [`BooleanArray`].
    /// # Panic
    /// This function panics iff `validity.len() != self.len()`.
    pub fn with_validity(&self, validity: Option<Bitmap>) -> Self {
        if matches!(&validity, Some(bitmap) if bitmap.len() != self.len()) {
            panic!("validity should be as least as large as the array")
        }
        let mut arr = self.clone();
        arr.validity = validity;
        arr
    }

    /// Try to convert this `BooleanArray` to a `MutableBooleanArray`
    pub fn into_mut(self) -> Either<Self, MutableBooleanArray> {
        use Either::*;

        if let Some(bitmap) = self.validity {
            match bitmap.into_mut() {
                Left(bitmap) => Left(BooleanArray::from_data(
                    self.data_type,
                    self.values,
                    Some(bitmap),
                )),
                Right(mutable_bitmap) => match self.values.into_mut() {
                    Left(immutable) => Left(BooleanArray::from_data(
                        self.data_type,
                        immutable,
                        Some(mutable_bitmap.into()),
                    )),
                    Right(mutable) => Right(MutableBooleanArray::from_data(
                        self.data_type,
                        mutable,
                        Some(mutable_bitmap),
                    )),
                },
            }
        } else {
            match self.values.into_mut() {
                Left(immutable) => Left(BooleanArray::from_data(self.data_type, immutable, None)),
                Right(mutable) => Right(MutableBooleanArray::from_data(
                    self.data_type,
                    mutable,
                    None,
                )),
            }
        }
    }
}

// accessors
impl BooleanArray {
    /// Returns the length of this array
    #[inline]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns the value at index `i`
    /// # Panic
    /// This function panics iff `i >= self.len()`.
    #[inline]
    pub fn value(&self, i: usize) -> bool {
        self.values.get_bit(i)
    }

    /// Returns the element at index `i` as bool
    /// # Safety
    /// Caller must be sure that `i < self.len()`
    #[inline]
    pub unsafe fn value_unchecked(&self, i: usize) -> bool {
        self.values.get_bit_unchecked(i)
    }

    /// The optional validity.
    #[inline]
    pub fn validity(&self) -> Option<&Bitmap> {
        self.validity.as_ref()
    }

    /// Returns the values of this [`BooleanArray`].
    #[inline]
    pub fn values(&self) -> &Bitmap {
        &self.values
    }
}

impl Array for BooleanArray {
    #[inline]
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    #[inline]
    fn len(&self) -> usize {
        self.len()
    }

    #[inline]
    fn data_type(&self) -> &DataType {
        &self.data_type
    }

    #[inline]
    fn validity(&self) -> Option<&Bitmap> {
        self.validity.as_ref()
    }

    #[inline]
    fn slice(&self, offset: usize, length: usize) -> Box<dyn Array> {
        Box::new(self.slice(offset, length))
    }
    #[inline]
    unsafe fn slice_unchecked(&self, offset: usize, length: usize) -> Box<dyn Array> {
        Box::new(self.slice_unchecked(offset, length))
    }
    fn with_validity(&self, validity: Option<Bitmap>) -> Box<dyn Array> {
        Box::new(self.with_validity(validity))
    }
}

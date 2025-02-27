//! Contains operators to filter arrays such as [`filter`].
use crate::array::growable::{make_growable, Growable};
use crate::bitmap::utils::{BitChunkIterExact, BitChunksExact};
use crate::bitmap::{utils::SlicesIterator, Bitmap, MutableBitmap};
use crate::chunk::Chunk;
use crate::datatypes::DataType;
use crate::error::Result;
use crate::types::simd::{NativeSimd, Simd};
use crate::types::BitChunkIter;
use crate::{array::*, types::NativeType};

/// Function that can filter arbitrary arrays
pub type Filter<'a> = Box<dyn Fn(&dyn Array) -> Box<dyn Array> + 'a + Send + Sync>;

/// # Safety
/// This assumes that the `mask_chunks` contains a number of set/true items equal
/// to `filter_count`
unsafe fn nonnull_filter_impl<T, I>(values: &[T], mut mask_chunks: I, filter_count: usize) -> Vec<T>
where
    T: NativeType + Simd,
    I: BitChunkIterExact<<<T as Simd>::Simd as NativeSimd>::Chunk>,
{
    let mut chunks = values.chunks_exact(T::Simd::LANES);

    let mut new = Vec::<T>::with_capacity(filter_count);
    let mut dst = new.as_mut_ptr();
    chunks
        .by_ref()
        .zip(mask_chunks.by_ref())
        .for_each(|(chunk, validity_chunk)| {
            let iter = BitChunkIter::new(validity_chunk, T::Simd::LANES);
            for (value, b) in chunk.iter().zip(iter) {
                if b {
                    unsafe {
                        dst.write(*value);
                        dst = dst.add(1);
                    };
                }
            }
        });

    chunks
        .remainder()
        .iter()
        .zip(mask_chunks.remainder_iter())
        .for_each(|(value, b)| {
            if b {
                unsafe {
                    dst.write(*value);
                    dst = dst.add(1);
                };
            }
        });

    unsafe { new.set_len(filter_count) };
    new
}

/// # Safety
/// This assumes that the `mask_chunks` contains a number of set/true items equal
/// to `filter_count`
unsafe fn null_filter_impl<T, I>(
    values: &[T],
    validity: &Bitmap,
    mut mask_chunks: I,
    filter_count: usize,
) -> (Vec<T>, MutableBitmap)
where
    T: NativeType + Simd,
    I: BitChunkIterExact<<<T as Simd>::Simd as NativeSimd>::Chunk>,
{
    let mut chunks = values.chunks_exact(T::Simd::LANES);

    let mut validity_chunks = validity.chunks::<<T::Simd as NativeSimd>::Chunk>();

    let mut new = Vec::<T>::with_capacity(filter_count);
    let mut new_validity = MutableBitmap::with_capacity(filter_count);
    let mut dst = new.as_mut_ptr();
    chunks
        .by_ref()
        .zip(validity_chunks.by_ref())
        .zip(mask_chunks.by_ref())
        .for_each(|((chunk, validity_chunk), mask_chunk)| {
            let mask_iter = BitChunkIter::new(mask_chunk, T::Simd::LANES);
            let validity_iter = BitChunkIter::new(validity_chunk, T::Simd::LANES);
            for ((value, is_valid), is_selected) in chunk.iter().zip(validity_iter).zip(mask_iter) {
                if is_selected {
                    unsafe {
                        dst.write(*value);
                        dst = dst.add(1);
                        new_validity.push_unchecked(is_valid);
                    };
                }
            }
        });

    chunks
        .remainder()
        .iter()
        .zip(validity_chunks.remainder_iter())
        .zip(mask_chunks.remainder_iter())
        .for_each(|((value, is_valid), is_selected)| {
            if is_selected {
                unsafe {
                    dst.write(*value);
                    dst = dst.add(1);
                    new_validity.push_unchecked(is_valid);
                };
            }
        });

    unsafe { new.set_len(filter_count) };
    (new, new_validity)
}

fn null_filter_simd<T: NativeType + Simd>(
    values: &[T],
    validity: &Bitmap,
    mask: &Bitmap,
) -> (Vec<T>, MutableBitmap) {
    assert_eq!(values.len(), mask.len());
    let filter_count = mask.len() - mask.null_count();

    let (slice, offset, length) = mask.as_slice();
    if offset == 0 {
        let mask_chunks = BitChunksExact::<<T::Simd as NativeSimd>::Chunk>::new(slice, length);
        unsafe { null_filter_impl(values, validity, mask_chunks, filter_count) }
    } else {
        let mask_chunks = mask.chunks::<<T::Simd as NativeSimd>::Chunk>();
        unsafe { null_filter_impl(values, validity, mask_chunks, filter_count) }
    }
}

fn nonnull_filter_simd<T: NativeType + Simd>(values: &[T], mask: &Bitmap) -> Vec<T> {
    assert_eq!(values.len(), mask.len());
    let filter_count = mask.len() - mask.null_count();

    let (slice, offset, length) = mask.as_slice();
    if offset == 0 {
        let mask_chunks = BitChunksExact::<<T::Simd as NativeSimd>::Chunk>::new(slice, length);
        unsafe { nonnull_filter_impl(values, mask_chunks, filter_count) }
    } else {
        let mask_chunks = mask.chunks::<<T::Simd as NativeSimd>::Chunk>();
        unsafe { nonnull_filter_impl(values, mask_chunks, filter_count) }
    }
}

fn filter_nonnull_primitive<T: NativeType + Simd>(
    array: &PrimitiveArray<T>,
    mask: &Bitmap,
) -> PrimitiveArray<T> {
    assert_eq!(array.len(), mask.len());

    if let Some(validity) = array.validity() {
        let (values, validity) = null_filter_simd(array.values(), validity, mask);
        PrimitiveArray::<T>::from_data(array.data_type().clone(), values.into(), validity.into())
    } else {
        let values = nonnull_filter_simd(array.values(), mask);
        PrimitiveArray::<T>::from_data(array.data_type().clone(), values.into(), None)
    }
}

fn filter_primitive<T: NativeType + Simd>(
    array: &PrimitiveArray<T>,
    mask: &BooleanArray,
) -> PrimitiveArray<T> {
    // todo: branch on mask.validity()
    filter_nonnull_primitive(array, mask.values())
}

fn filter_growable<'a>(growable: &mut impl Growable<'a>, chunks: &[(usize, usize)]) {
    chunks
        .iter()
        .for_each(|(start, len)| growable.extend(0, *start, *len));
}

/// Returns a prepared function optimized to filter multiple arrays.
/// Creating this function requires time, but using it is faster than [filter] when the
/// same filter needs to be applied to multiple arrays (e.g. a multiple columns).
pub fn build_filter(filter: &BooleanArray) -> Result<Filter> {
    let iter = SlicesIterator::new(filter.values());
    let filter_count = iter.slots();
    let chunks = iter.collect::<Vec<_>>();

    use crate::datatypes::PhysicalType::*;
    Ok(Box::new(move |array: &dyn Array| {
        match array.data_type().to_physical_type() {
            Primitive(primitive) => with_match_primitive_type!(primitive, |$T| {
                let array = array.as_any().downcast_ref().unwrap();
                let mut growable =
                    growable::GrowablePrimitive::<$T>::new(vec![array], false, filter_count);
                filter_growable(&mut growable, &chunks);
                let array: PrimitiveArray<$T> = growable.into();
                Box::new(array)
            }),
            Utf8 => {
                let array = array.as_any().downcast_ref::<Utf8Array<i32>>().unwrap();
                let mut growable = growable::GrowableUtf8::new(vec![array], false, filter_count);
                filter_growable(&mut growable, &chunks);
                let array: Utf8Array<i32> = growable.into();
                Box::new(array)
            }
            LargeUtf8 => {
                let array = array.as_any().downcast_ref::<Utf8Array<i64>>().unwrap();
                let mut growable = growable::GrowableUtf8::new(vec![array], false, filter_count);
                filter_growable(&mut growable, &chunks);
                let array: Utf8Array<i64> = growable.into();
                Box::new(array)
            }
            _ => {
                let mut mutable = make_growable(&[array], false, filter_count);
                chunks
                    .iter()
                    .for_each(|(start, len)| mutable.extend(0, *start, *len));
                mutable.as_box()
            }
        }
    }))
}

/// Filters an [Array], returning elements matching the filter (i.e. where the values are true).
///
/// Note that the nulls of `filter` are interpreted as `false` will lead to these elements being
/// masked out.
///
/// # Example
/// ```rust
/// # use arrow2::array::{Int32Array, PrimitiveArray, BooleanArray};
/// # use arrow2::error::Result;
/// # use arrow2::compute::filter::filter;
/// # fn main() -> Result<()> {
/// let array = PrimitiveArray::from_slice([5, 6, 7, 8, 9]);
/// let filter_array = BooleanArray::from_slice(&vec![true, false, false, true, false]);
/// let c = filter(&array, &filter_array)?;
/// let c = c.as_any().downcast_ref::<Int32Array>().unwrap();
/// assert_eq!(c, &PrimitiveArray::from_slice(vec![5, 8]));
/// # Ok(())
/// # }
/// ```
pub fn filter(array: &dyn Array, filter: &BooleanArray) -> Result<Box<dyn Array>> {
    // The validities may be masking out `true` bits, making the filter operation
    // based on the values incorrect
    if let Some(validities) = filter.validity() {
        let values = filter.values();
        let new_values = values & validities;
        let filter = BooleanArray::from_data(DataType::Boolean, new_values, None);
        return crate::compute::filter::filter(array, &filter);
    }

    use crate::datatypes::PhysicalType::*;
    match array.data_type().to_physical_type() {
        Primitive(primitive) => with_match_primitive_type!(primitive, |$T| {
            let array = array.as_any().downcast_ref().unwrap();
            Ok(Box::new(filter_primitive::<$T>(array, filter)))
        }),
        _ => {
            let iter = SlicesIterator::new(filter.values());
            let mut mutable = make_growable(&[array], false, iter.slots());
            iter.for_each(|(start, len)| mutable.extend(0, start, len));
            Ok(mutable.as_box())
        }
    }
}

/// Returns a new [Chunk] with arrays containing only values matching the filter.
/// This is a convenience function: filter multiple columns is embarassingly parallel.
pub fn filter_chunk<A: AsRef<dyn Array>>(
    columns: &Chunk<A>,
    filter_values: &BooleanArray,
) -> Result<Chunk<Box<dyn Array>>> {
    let arrays = columns.arrays();

    let num_colums = arrays.len();

    let filtered_arrays = match num_colums {
        1 => {
            vec![filter(columns.arrays()[0].as_ref(), filter_values)?]
        }
        _ => {
            let filter = build_filter(filter_values)?;
            arrays.iter().map(|a| filter(a.as_ref())).collect()
        }
    };
    Chunk::try_new(filtered_arrays)
}

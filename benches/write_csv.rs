use std::sync::Arc;

use criterion::{criterion_group, criterion_main, Criterion};

use arrow2::array::*;
use arrow2::chunk::Chunk;
use arrow2::error::Result;
use arrow2::io::csv::write;
use arrow2::util::bench_util::*;

type ChunkArc = Chunk<Arc<dyn Array>>;

fn write_batch(columns: &ChunkArc) -> Result<()> {
    let writer = &mut write::WriterBuilder::new().from_writer(vec![]);

    assert_eq!(columns.arrays().len(), 1);
    write::write_header(writer, &["a"])?;

    let options = write::SerializeOptions::default();
    write::write_chunk(writer, columns, &options)
}

fn make_chunk(array: impl Array + 'static) -> Chunk<Arc<dyn Array>> {
    Chunk::new(vec![Arc::new(array)])
}

fn add_benchmark(c: &mut Criterion) {
    (10..=18).step_by(2).for_each(|log2_size| {
        let size = 2usize.pow(log2_size);

        let array = create_primitive_array::<i32>(size, 0.1);
        let batch = make_chunk(array);

        c.bench_function(&format!("csv write i32 2^{}", log2_size), |b| {
            b.iter(|| write_batch(&batch))
        });

        let array = create_string_array::<i32>(size, 100, 0.1, 42);
        let batch = make_chunk(array);

        c.bench_function(&format!("csv write utf8 2^{}", log2_size), |b| {
            b.iter(|| write_batch(&batch))
        });

        let array = create_primitive_array::<f64>(size, 0.1);
        let batch = make_chunk(array);

        c.bench_function(&format!("csv write f64 2^{}", log2_size), |b| {
            b.iter(|| write_batch(&batch))
        });
    });
}

criterion_group!(benches, add_benchmark);
criterion_main!(benches);

use std::collections::VecDeque;
use std::io::{Read, Seek};

use crate::array::MapArray;
use crate::buffer::Buffer;
use crate::datatypes::DataType;
use crate::error::{ArrowError, Result};

use super::super::super::IpcField;
use super::super::deserialize::{read, skip};
use super::super::read_basic::*;
use super::super::Dictionaries;
use super::super::{Compression, IpcBuffer, Node, Version};

#[allow(clippy::too_many_arguments)]
pub fn read_map<R: Read + Seek>(
    field_nodes: &mut VecDeque<Node>,
    data_type: DataType,
    ipc_field: &IpcField,
    buffers: &mut VecDeque<IpcBuffer>,
    reader: &mut R,
    dictionaries: &Dictionaries,
    block_offset: u64,
    is_little_endian: bool,
    compression: Option<Compression>,
    version: Version,
) -> Result<MapArray> {
    let field_node = field_nodes.pop_front().ok_or_else(|| {
        ArrowError::oos(format!(
            "IPC: unable to fetch the field for {:?}. The file or stream is corrupted.",
            data_type
        ))
    })?;

    let validity = read_validity(
        buffers,
        field_node,
        reader,
        block_offset,
        is_little_endian,
        compression,
    )?;

    let offsets = read_buffer::<i32, _>(
        buffers,
        1 + field_node.length() as usize,
        reader,
        block_offset,
        is_little_endian,
        compression,
    )
    // Older versions of the IPC format sometimes do not report an offset
    .or_else(|_| Result::Ok(Buffer::<i32>::from(vec![0i32])))?;

    let field = MapArray::get_field(&data_type);

    let field = read(
        field_nodes,
        field,
        &ipc_field.fields[0],
        buffers,
        reader,
        dictionaries,
        block_offset,
        is_little_endian,
        compression,
        version,
    )?;
    Ok(MapArray::from_data(data_type, offsets, field, validity))
}

pub fn skip_map(
    field_nodes: &mut VecDeque<Node>,
    data_type: &DataType,
    buffers: &mut VecDeque<IpcBuffer>,
) -> Result<()> {
    let _ = field_nodes.pop_front().ok_or_else(|| {
        ArrowError::oos("IPC: unable to fetch the field for map. The file or stream is corrupted.")
    })?;

    let _ = buffers
        .pop_front()
        .ok_or_else(|| ArrowError::oos("IPC: missing validity buffer."))?;
    let _ = buffers
        .pop_front()
        .ok_or_else(|| ArrowError::oos("IPC: missing offsets buffer."))?;

    let data_type = MapArray::get_field(data_type).data_type();

    skip(field_nodes, data_type, buffers)
}

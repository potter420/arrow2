use super::{DataType, Metadata};

/// Represents Arrow's metadata of a "column".
///
/// A [`Field`] is the closest representation of the traditional "column": a logical type
/// ([`DataType`]) with a name and nullability.
/// A Field has optional [`Metadata`] that can be used to annotate the field with custom metadata.
///
/// Almost all IO in this crate uses [`Field`] to represent logical information about the data
/// to be serialized.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Field {
    /// Its name
    pub name: String,
    /// Its logical [`DataType`]
    pub data_type: DataType,
    /// Its nullability
    pub is_nullable: bool,
    /// Additional custom (opaque) metadata.
    pub metadata: Metadata,
}

impl Field {
    /// Creates a new [`Field`].
    pub fn new<T: Into<String>>(name: T, data_type: DataType, is_nullable: bool) -> Self {
        Field {
            name: name.into(),
            data_type,
            is_nullable,
            metadata: Default::default(),
        }
    }

    /// Creates a new [`Field`] with metadata.
    #[inline]
    pub fn with_metadata(self, metadata: Metadata) -> Self {
        Self {
            name: self.name,
            data_type: self.data_type,
            is_nullable: self.is_nullable,
            metadata,
        }
    }

    /// Returns the [`Field`]'s [`DataType`].
    #[inline]
    pub fn data_type(&self) -> &DataType {
        &self.data_type
    }
}

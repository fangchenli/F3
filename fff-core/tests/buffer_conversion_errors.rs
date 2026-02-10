/// Buffer to array conversion error tests for fff-core
///
/// Tests validate proper error handling when buffer_to_array functions
/// receive insufficient buffers (previously caused panics via unwrap).

#[cfg(test)]
mod buffer_size_tests {
    use arrow_buffer::Buffer;
    use arrow_schema::DataType;
    use bytes::BytesMut;
    use fff_core::util::buffer_to_array::{
        new_primitive_array, primitive_array_from_arrow_buffers, primitive_array_from_buffers,
    };

    #[test]
    fn test_primitive_array_empty_buffers() {
        // Passing an empty buffer list should return an error, not panic
        let buffers: Vec<BytesMut> = vec![];
        let result =
            new_primitive_array::<arrow_array::types::Int32Type>(buffers, 10, &DataType::Int32);
        assert!(result.is_err(), "Empty buffer list should return error");
    }

    #[test]
    fn test_primitive_array_missing_data_buffer() {
        // Passing only validity buffer (missing data buffer) should return error
        let buffers: Vec<BytesMut> = vec![BytesMut::new()]; // only validity
        let result =
            new_primitive_array::<arrow_array::types::Int32Type>(buffers, 0, &DataType::Int32);
        assert!(result.is_err(), "Missing data buffer should return error");
    }

    #[test]
    fn test_primitive_array_from_arrow_buffers_empty() {
        let buffers: Vec<Buffer> = vec![];
        let result = primitive_array_from_arrow_buffers(&DataType::Int32, buffers, 10);
        assert!(
            result.is_err(),
            "Empty arrow buffer list should return error"
        );
    }

    #[test]
    fn test_primitive_array_from_buffers_empty() {
        let buffers: Vec<BytesMut> = vec![];
        let result = primitive_array_from_buffers(&DataType::Int32, buffers, 10);
        assert!(result.is_err(), "Empty buffer list should return error");
    }

    #[test]
    fn test_boolean_array_missing_data_buffer() {
        // Boolean type has its own branch - test it specifically
        let buffers: Vec<Buffer> = vec![Buffer::from_vec(vec![0u8])]; // only validity
        let result = primitive_array_from_arrow_buffers(&DataType::Boolean, buffers, 1);
        assert!(
            result.is_err(),
            "Boolean with missing data buffer should return error"
        );
    }

    #[test]
    fn test_successful_primitive_array() {
        // Verify that valid buffers still work correctly
        let validity = BytesMut::new(); // empty = all valid
        let mut data = BytesMut::with_capacity(12);
        data.extend_from_slice(&1i32.to_le_bytes());
        data.extend_from_slice(&2i32.to_le_bytes());
        data.extend_from_slice(&3i32.to_le_bytes());

        let buffers = vec![validity, data];
        let result =
            new_primitive_array::<arrow_array::types::Int32Type>(buffers, 3, &DataType::Int32);
        assert!(result.is_ok(), "Valid buffers should succeed");
        let array = result.unwrap();
        assert_eq!(array.len(), 3);
    }
}

#[cfg(test)]
mod byte_array_tests {
    use bytes::BytesMut;
    use fff_core::util::buffer_to_array::new_generic_byte_array;

    #[test]
    fn test_generic_byte_array_empty_buffers() {
        let buffers: Vec<BytesMut> = vec![];
        let result =
            new_generic_byte_array::<arrow_array::types::GenericStringType<i32>>(buffers, 10);
        assert!(result.is_err(), "Empty buffer list should return error");
    }

    #[test]
    fn test_generic_byte_array_missing_indices() {
        // Only validity buffer provided, missing indices
        let buffers: Vec<BytesMut> = vec![BytesMut::new()];
        let result =
            new_generic_byte_array::<arrow_array::types::GenericStringType<i32>>(buffers, 0);
        assert!(
            result.is_err(),
            "Missing indices buffer should return error"
        );
    }
}

#[cfg(test)]
mod byte_view_tests {
    use arrow_buffer::Buffer;
    use fff_core::util::buffer_to_array::new_generic_byte_view_array_from_arrow_buffer_iter;

    #[test]
    fn test_byte_view_array_empty_buffers() {
        let buffers: Vec<Buffer> = vec![];
        let result = new_generic_byte_view_array_from_arrow_buffer_iter::<
            arrow_array::types::StringViewType,
        >(buffers.into_iter(), 10);
        assert!(result.is_err(), "Empty buffer list should return error");
    }

    #[test]
    fn test_byte_view_array_missing_views() {
        // Only validity buffer provided, missing views
        let buffers: Vec<Buffer> = vec![Buffer::from_vec(vec![0u8])];
        let result = new_generic_byte_view_array_from_arrow_buffer_iter::<
            arrow_array::types::StringViewType,
        >(buffers.into_iter(), 1);
        assert!(result.is_err(), "Missing views buffer should return error");
    }
}

#[cfg(test)]
mod list_tests {
    use arrow_buffer::Buffer;
    use bytes::BytesMut;
    use fff_core::util::buffer_to_array::{
        new_list_offsets_validity, new_list_offsets_validity_from_buffers,
    };
    use std::sync::Arc;

    #[test]
    fn test_list_offsets_empty_buffers() {
        let buffers: Vec<BytesMut> = vec![];
        let child = Arc::new(arrow_schema::Field::new(
            "item",
            arrow_schema::DataType::Int32,
            true,
        ));
        let result = new_list_offsets_validity::<arrow_array::types::Int32Type>(buffers, 10, child);
        assert!(result.is_err(), "Empty buffer list should return error");
    }

    #[test]
    fn test_list_offsets_from_buffers_empty() {
        let buffers: Vec<Buffer> = vec![];
        let result = new_list_offsets_validity_from_buffers::<arrow_array::types::Int32Type>(
            buffers, 10, None,
        );
        assert!(result.is_err(), "Empty buffer list should return error");
    }

    #[test]
    fn test_list_offsets_missing_data_buffer() {
        // Only validity buffer provided
        let buffers: Vec<Buffer> = vec![Buffer::from_vec(vec![0u8])];
        let result = new_list_offsets_validity_from_buffers::<arrow_array::types::Int32Type>(
            buffers, 1, None,
        );
        assert!(
            result.is_err(),
            "Missing offsets buffer should return error"
        );
    }
}

#[cfg(test)]
mod unsupported_type_tests {
    use arrow_buffer::Buffer;
    use arrow_schema::DataType;
    use bytes::BytesMut;
    use fff_core::util::buffer_to_array::{
        primitive_array_from_arrow_buffers, primitive_array_from_buffers,
    };

    #[test]
    fn test_unsupported_type_arrow_buffers() {
        let buffers: Vec<Buffer> = vec![Buffer::from_vec(vec![0u8])];
        let result = primitive_array_from_arrow_buffers(
            &DataType::Struct(arrow_schema::Fields::empty()),
            buffers,
            1,
        );
        assert!(
            result.is_err(),
            "Struct type should return unsupported error"
        );
    }

    #[test]
    fn test_unsupported_type_bytesmut() {
        let buffers: Vec<BytesMut> = vec![BytesMut::new()];
        let result = primitive_array_from_buffers(
            &DataType::Struct(arrow_schema::Fields::empty()),
            buffers,
            1,
        );
        assert!(
            result.is_err(),
            "Struct type should return unsupported error"
        );
    }
}

// Placeholder removed - real tests now populate this file

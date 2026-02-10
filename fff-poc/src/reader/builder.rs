use crate::{
    common::checksum::{create_checksum, ChecksumType},
    context::{WASMId, WASMReadingContext},
    dict::shared_dictionary_cache::SharedDictionaryCache,
    file::footer::{parse_footer, MetadataSection},
    io::reader::Reader,
    options::DEFAULT_IOUNIT_SIZE,
    reader::{read_postscript, RowGroupCntNPointer},
};
use arrow_buffer::MutableBuffer;
use bytes::Bytes;
use fff_core::errors::{Error, Result};
use fff_format::File::fff::flatbuf::root_as_footer;
use fff_format::POSTSCRIPT_SIZE;
use fff_ude_wasm::Runtime;
use std::{collections::HashMap, sync::Arc};

use crate::reader::{FileReaderV2, Projection, Selection};

pub struct FileReaderV2Builder<R: Reader + Clone> {
    reader: R,
    projections: Projection,
    selection: Selection,
    /// Whether we do a first 8MB read to the footer at once?
    read_ahead: bool,
    wasm_rts: Option<HashMap<WASMId, Arc<Runtime>>>,
    /// Whether we verify the IOUnit checksum.
    verify_io_unit_checksum: bool,
    /// Whether we verify the file checksum.
    verify_file_checksum: bool,
}

impl<R: Reader + Clone> FileReaderV2Builder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            projections: Projection::default(),
            selection: Selection::default(),
            read_ahead: false,
            wasm_rts: None,
            verify_io_unit_checksum: false,
            verify_file_checksum: false,
        }
    }

    pub fn with_projections(mut self, projections: Projection) -> Self {
        self.projections = projections;
        self
    }

    pub fn with_selection(mut self, selection: Selection) -> Result<Self> {
        if let Selection::RowIndexes(row_indexes) = &selection {
            if row_indexes.len() != 1 {
                return Err(Error::General(
                    "Only one row index is supported for experiment purposes".to_string(),
                ));
            }
        }
        self.selection = selection;
        Ok(self)
    }

    pub fn with_read_ahead(mut self, read_ahead: bool) -> Self {
        self.read_ahead = read_ahead;
        self
    }

    /// Init the file reader using the existing Wasm Runtime provided, instead of compiling from the Wasm in the file.
    pub fn with_existing_runtimes(mut self, wasm_rts: HashMap<WASMId, Arc<Runtime>>) -> Self {
        self.wasm_rts = Some(wasm_rts);
        self
    }

    /// Whether we verify the IOUnit checksum.
    pub fn with_verify_io_unit_checksum(mut self, verify_io_unit_checksum: bool) -> Self {
        self.verify_io_unit_checksum = verify_io_unit_checksum;
        self
    }

    /// Whether we verify the file checksum.
    pub fn with_verify_file_checksum(mut self, verify_file_checksum: bool) -> Self {
        self.verify_file_checksum = verify_file_checksum;
        self
    }

    fn verify_file_checksum(
        &self,
        file_size: u64,
        checksum_in_ps: u64,
        checksum_type: ChecksumType,
    ) -> Result<()> {
        let mut checksum_calculator = create_checksum(&checksum_type);
        let data_exclude_ps = {
            let len = file_size - POSTSCRIPT_SIZE;
            let mut data_exclude_ps = MutableBuffer::from_len_zeroed(len as usize);
            self.reader
                .read_exact_at(data_exclude_ps.as_slice_mut(), 0)?;
            data_exclude_ps
        };
        checksum_calculator.update(data_exclude_ps.as_slice());
        let computed_checksum = checksum_calculator.finalize();
        if checksum_in_ps != computed_checksum {
            return Err(Error::General(
                "File level Checksum verification failed".to_string(),
            ));
        }
        Ok(())
    }

    pub fn build(self) -> Result<FileReaderV2<R>> {
        let file_size = self.reader.size()?;
        let read_ahead_buffer = if self.read_ahead {
            let len = std::cmp::min(DEFAULT_IOUNIT_SIZE, file_size) as usize;
            let mut read_ahead_buffer = MutableBuffer::from_len_zeroed(len);
            self.reader
                .read_exact_at(read_ahead_buffer.as_slice_mut(), file_size - len as u64)?;
            read_ahead_buffer
        } else {
            MutableBuffer::new(0)
        };
        let post_script = if self.read_ahead {
            read_postscript(read_ahead_buffer.as_slice(), read_ahead_buffer.len() as u64)?
        } else {
            read_postscript(&self.reader, file_size)?
        };
        if self.verify_file_checksum {
            // TODO: if verification succeeds, we can reuse the data_exclude_ps buffer.
            self.verify_file_checksum(
                file_size,
                post_script.data_checksum,
                post_script.checksum_type,
            )?;
        }
        let mut footer_buffer = MutableBuffer::from_len_zeroed(post_script.footer_size as usize);
        let footer_fbs = if self.read_ahead {
            if post_script.footer_size >= (DEFAULT_IOUNIT_SIZE - 32) as u32 {
                return Err(Error::General(format!(
                    "Footer size {} exceeds read-ahead buffer capacity (max {})",
                    post_script.footer_size,
                    (DEFAULT_IOUNIT_SIZE - 32) as u32
                )));
            }
            root_as_footer(
                &read_ahead_buffer.as_slice()[read_ahead_buffer.len()
                    - POSTSCRIPT_SIZE as usize
                    - post_script.footer_size as usize
                    ..read_ahead_buffer.len() - POSTSCRIPT_SIZE as usize],
            )
            .map_err(|e| Error::ParseError(format!("Unable to get root as footer: {e:?}")))?
        } else {
            self.reader.read_exact_at(
                footer_buffer.as_slice_mut(),
                file_size - POSTSCRIPT_SIZE - post_script.footer_size as u64,
            )?;
            root_as_footer(&footer_buffer)
                .map_err(|e| Error::ParseError(format!("Unable to get root as footer: {e:?}")))?
        };
        // FIXME: use logical tree to know which logical encoding to use.
        let (
            schema,
            _logical_tree,
            row_groups_pointer,
            shared_dict_table,
            optional_sections,
            encoding_versions,
        ) = parse_footer(&footer_fbs)?;
        // Depending on the ratio between number of projected columns and total columns,
        // we fetch them all or do one by one fetch.
        let rg_metadatas = row_groups_pointer.row_group_metadatas().ok_or_else(|| {
            Error::ParseError("Row group metadatas not found in footer".to_string())
        })?;
        let first_rg = rg_metadatas.get(0);
        let first_col_metas = first_rg.col_metadatas().ok_or_else(|| {
            Error::ParseError("Column metadatas not found in first row group".to_string())
        })?;
        let total_columns = first_col_metas.len();
        // TODO: we can use Selection to skip reading metadata of some row groups.
        // This requires mapping Selection to the correct selection indices after pruning row groups.
        let row_counts = row_groups_pointer.row_counts().ok_or_else(|| {
            Error::ParseError("Row counts not found in row groups pointer".to_string())
        })?;
        let offsets = row_groups_pointer.offsets().ok_or_else(|| {
            Error::ParseError("Offsets not found in row groups pointer".to_string())
        })?;
        let sizes = row_groups_pointer.sizes().ok_or_else(|| {
            Error::ParseError("Sizes not found in row groups pointer".to_string())
        })?;
        let row_group_cnt_n_pointers =
            itertools::izip!(row_counts.iter(), offsets.iter(), sizes.iter())
                .map(|(row_count, offset, size)| RowGroupCntNPointer {
                    row_count,
                    _offset: offset,
                    _size: size,
                })
                .collect();
        let ratio = match &self.projections {
            Projection::All => 1.0,
            Projection::LeafColumnIndexes(projections) => {
                projections.len() as f64 / total_columns as f64
            }
        };
        // let all_metadata_buffer = if false {
        let all_metadata_buffer = if ratio > 0.6 || total_columns <= 100 {
            let mut res: Vec<u8> =
                vec![0; post_script.metadata_size as usize - post_script.footer_size as usize];
            if self.read_ahead {
                read_ahead_buffer.read_exact_at(
                    &mut res,
                    read_ahead_buffer.len() as u64
                        - POSTSCRIPT_SIZE
                        - post_script.metadata_size as u64,
                )?;
            } else {
                self.reader.read_exact_at(
                    &mut res,
                    file_size - POSTSCRIPT_SIZE - post_script.metadata_size as u64,
                )?;
            }
            Some(Bytes::from(res))
        } else {
            None
        };
        let row_group_metadata_fbs = row_groups_pointer
            .row_group_metadatas()
            .ok_or_else(|| Error::ParseError("Row group metadatas not found".to_string()))?;
        let mut grouped_column_metadata_buffers: Vec<Vec<Bytes>> = vec![];
        for rg_meta_fbs in row_group_metadata_fbs.iter() {
            let mut column_metadata_buffers: Vec<Bytes> = vec![];
            let col_metadatas = rg_meta_fbs.col_metadatas().ok_or_else(|| {
                Error::ParseError("Column metadatas not found in row group".to_string())
            })?;
            let column_meta_ptrs = match self.projections {
                Projection::All => col_metadatas.into_iter().collect(),
                Projection::LeafColumnIndexes(ref projections) => {
                    let mut column_meta_offsets = vec![];
                    for i in projections {
                        column_meta_offsets.push(col_metadatas.get(*i));
                    }
                    column_meta_offsets
                }
            };
            for column_meta_pointer in column_meta_ptrs {
                match all_metadata_buffer {
                    None => {
                        // read each column meta one by one
                        let column_meta_size = column_meta_pointer.size_() as usize;
                        let mut column_meta_buffer: Vec<u8> = vec![0; column_meta_size];
                        self.reader
                            .read_exact_at(&mut column_meta_buffer, column_meta_pointer.offset())?;
                        column_metadata_buffers.push(column_meta_buffer.into());
                    }
                    Some(ref buf) => {
                        // column metas are already read at once
                        let data_size = file_size as usize
                            - POSTSCRIPT_SIZE as usize
                            - post_script.metadata_size as usize;
                        column_metadata_buffers.push(buf.slice(
                            column_meta_pointer.offset() as usize - data_size
                                ..column_meta_pointer.offset() as usize - data_size
                                    + column_meta_pointer.size_() as usize,
                        ))
                    }
                }
            }
            grouped_column_metadata_buffers.push(column_metadata_buffers);
        }
        let wasm_context = if let Some(wasm_rts) = self.wasm_rts {
            Some(WASMReadingContext::new_with_rt_and_versions(wasm_rts, encoding_versions).into())
        } else {
            match optional_sections {
                Some(sections) => {
                    let names = sections.names().ok_or_else(|| {
                        Error::ParseError("Optional section names not found".to_string())
                    })?;
                    let pos = names
                        .iter()
                        .position(|v| v == "WASMBinaries")
                        .ok_or_else(|| {
                            Error::General(
                                "WASMBinaries section not found in optional sections".to_string(),
                            )
                        })?;
                    let offsets = sections.offsets().ok_or_else(|| {
                        Error::ParseError("Optional section offsets not found".to_string())
                    })?;
                    let sizes = sections.sizes().ok_or_else(|| {
                        Error::ParseError("Optional section sizes not found".to_string())
                    })?;
                    let compression_types = sections.compression_types().ok_or_else(|| {
                        Error::ParseError(
                            "Optional section compression types not found".to_string(),
                        )
                    })?;
                    Some(
                        WASMReadingContext::new_with_versions(
                            MetadataSection {
                                offset: offsets.get(pos),
                                size: sizes.get(pos),
                                compression_type: compression_types.get(pos),
                            },
                            self.reader.clone(),
                            encoding_versions,
                        )
                        .into(),
                    )
                }
                None => None,
            }
        };
        let shared_dictionary_cache = match shared_dict_table {
            Some(shared_dict_table) => Some(SharedDictionaryCache::try_new_read_all(
                self.reader.clone(),
                shared_dict_table,
                wasm_context.clone(),
            )?),
            None => None,
        };
        Ok(FileReaderV2 {
            reader: self.reader,
            schema: schema.into(),
            projections: self.projections,
            selection: self.selection,
            grouped_column_metadata_buffers,
            row_group_cnt_n_pointers,
            wasm_context,
            shared_dictionary_cache,
            checksum_type: self
                .verify_io_unit_checksum
                .then_some(post_script.checksum_type),
        })
    }
}

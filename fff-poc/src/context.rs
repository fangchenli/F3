use std::{
    collections::HashMap,
    path::PathBuf,
    rc::Rc,
    sync::{Arc, OnceLock},
};

use arrow_schema::DataType;
use tracing::{debug, error, info, instrument, warn};
use fff_format::File::fff::flatbuf as fb;
use fff_test_util::BUILTIN_WASM_PATH;
use fff_ude_wasm::Runtime;
use semver::Version;

use crate::{file::footer::MetadataSection, io::reader::Reader};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct WASMId(pub u32);

#[derive(Debug, PartialEq, Clone)]
pub struct WasmLib {
    encode_lib_path: Rc<PathBuf>,
    decode_wasm_binary: Rc<Vec<u8>>,
}

impl WasmLib {
    pub fn new(enc_path: PathBuf, dec_wasm: Vec<u8>) -> Self {
        Self {
            encode_lib_path: Rc::new(enc_path),
            decode_wasm_binary: Rc::new(dec_wasm),
        }
    }

    pub fn encode_lib_path(&self) -> Rc<PathBuf> {
        self.encode_lib_path.clone()
    }
}

/// Behavior is a little weird for the research use now. We either use default_with_always_set_custom_wasm() to write all built-in as wasm,
/// or we set built-in as native and allow custom wasm.
#[derive(Debug)]
pub struct WASMWritingContext {
    /// WASMId to its binaries
    wasms: HashMap<WASMId, WasmLib>,
    /// DataType to its WASMId
    data_type_to_wasm_id: HashMap<DataType, WASMId>,
    /// Always write CUSTOM_WASM encoding, this is mainly for testing
    always_set_custom_wasm_for_built_in: bool,
    /// WasmId for built-in
    builtin_wasm_id: Option<WASMId>,
}

impl Default for WASMWritingContext {
    fn default() -> Self {
        // Built-in WASM decoder path is now configurable via FFF_BUILTIN_WASM_PATH env var
        let wasm_path = BUILTIN_WASM_PATH.as_path();
        debug!(?wasm_path, "Loading built-in WASM decoder");

        let wasm_binary = match std::fs::read(wasm_path) {
            Ok(data) => {
                info!(path = ?wasm_path, size = data.len(), "Successfully loaded built-in WASM decoder");
                data
            }
            Err(e) => {
                error!(path = ?wasm_path, error = %e, "Failed to load built-in WASM decoder");
                panic!("Failed to load WASM decoder: {}", e);
            }
        };

        Self {
            wasms: HashMap::from([(
                WASMId(0),
                WasmLib {
                    encode_lib_path: BUILTIN_WASM_PATH.clone().into(),
                    decode_wasm_binary: wasm_binary.into(),
                },
            )]),
            data_type_to_wasm_id: HashMap::default(),
            always_set_custom_wasm_for_built_in: false,
            builtin_wasm_id: Some(WASMId(0)),
        }
    }
}

impl WASMWritingContext {
    pub fn default_with_always_set_custom_wasm() -> Self {
        Self {
            always_set_custom_wasm_for_built_in: true,
            ..Self::default()
        }
    }

    pub fn empty() -> Self {
        Self {
            wasms: HashMap::new(),
            data_type_to_wasm_id: HashMap::new(),
            always_set_custom_wasm_for_built_in: false,
            builtin_wasm_id: None,
        }
    }

    /// Built-in Wasm will not be included in the final WasmIds
    pub fn with_custom_wasms(
        wasms: HashMap<WASMId, WasmLib>,
        data_type_to_wasm_id: HashMap<DataType, WASMId>,
    ) -> Self {
        Self {
            wasms,
            data_type_to_wasm_id,
            always_set_custom_wasm_for_built_in: false,
            builtin_wasm_id: None,
        }
    }

    pub fn get_sorted_wasms(&self) -> Vec<&[u8]> {
        let mut wasms = self.wasms.iter().collect::<Vec<_>>();
        wasms.sort_by_key(|(k, _)| k.0);
        wasms
            .into_iter()
            .map(|(_, v)| v.decode_wasm_binary.as_slice())
            .collect()
    }

    pub fn data_type_to_wasm_id(&self, dt: &DataType) -> Option<WASMId> {
        self.data_type_to_wasm_id.get(dt).copied()
    }

    pub fn data_type_to_wasm_lib(&self, dt: &DataType) -> Option<WasmLib> {
        self.data_type_to_wasm_id
            .get(dt)
            .and_then(|x| self.wasms.get(x).cloned())
    }

    pub fn always_set_custom_wasm_for_built_in(&self) -> bool {
        self.always_set_custom_wasm_for_built_in
    }

    pub fn builtin_wasm_id(&self) -> Option<WASMId> {
        self.builtin_wasm_id
    }
}

pub struct WASMReadingContext<R> {
    /// runtime - stores Result to handle initialization errors
    lazy_wasm: OnceLock<fff_core::errors::Result<HashMap<WASMId, Arc<Runtime>>>>,
    wasm_locations: Option<MetadataSection>,
    r: Option<R>,
    /// Mapping of encoding types to their semantic versions
    encoding_versions: Option<HashMap<fb::EncodingType, Version>>,
}

impl<R: Reader> WASMReadingContext<R> {
    // Private constructor to reduce code duplication
    fn new_internal(
        lazy_wasm: OnceLock<fff_core::errors::Result<HashMap<WASMId, Arc<Runtime>>>>,
        wasm_locations: Option<MetadataSection>,
        r: Option<R>,
        encoding_versions: Option<HashMap<fb::EncodingType, Version>>,
    ) -> Self {
        Self {
            lazy_wasm,
            wasm_locations,
            r,
            encoding_versions,
        }
    }

    // For lazy loading from file
    pub fn new(wasm_locations: MetadataSection, r: R) -> Self {
        Self::new_with_versions(wasm_locations, r, None)
    }

    pub fn new_with_versions(
        wasm_locations: MetadataSection,
        r: R,
        encoding_versions: Option<HashMap<fb::EncodingType, Version>>,
    ) -> Self {
        Self::new_internal(
            OnceLock::new(),
            Some(wasm_locations),
            Some(r),
            encoding_versions,
        )
    }

    // For pre-built runtimes
    pub fn new_with_rt(wasm_rts: HashMap<WASMId, Arc<Runtime>>) -> Self {
        Self::new_with_rt_and_versions(wasm_rts, None)
    }

    pub fn new_with_rt_and_versions(
        wasm_rts: HashMap<WASMId, Arc<Runtime>>,
        encoding_versions: Option<HashMap<fb::EncodingType, Version>>,
    ) -> Self {
        let lazy_wasm = OnceLock::new();
        lazy_wasm.get_or_init(|| Ok(wasm_rts));
        Self::new_internal(lazy_wasm, None, None, encoding_versions)
    }

    #[instrument(skip(self), fields(wasm_id = wasm_id.0))]
    pub fn get_runtime(&self, wasm_id: WASMId) -> fff_core::errors::Result<Arc<Runtime>> {
        let runtimes_result = self.lazy_wasm.get_or_init(|| {
            debug!("Initializing WASM runtimes from file");

            // Helper closure to handle initialization with proper error handling
            let init = || -> fff_core::errors::Result<HashMap<WASMId, Arc<Runtime>>> {
                let wasm_locations = self.wasm_locations.as_ref()
                    .ok_or_else(|| fff_core::errors::Error::General("WASM locations not available".to_string()))?;
                let read = self.r.as_ref()
                    .ok_or_else(|| fff_core::errors::Error::General("Reader not available".to_string()))?;

                let mut buf = vec![0; wasm_locations.size as usize];
                debug!(offset = wasm_locations.offset, size = wasm_locations.size, "Reading WASM binaries metadata");
                read.read_exact_at(&mut buf, wasm_locations.offset)?;

                let wasm_binaries = flatbuffers::root::<fb::WASMBinaries>(&buf)
                    .map_err(|e| fff_core::errors::Error::General(format!("Failed to parse WASM binaries metadata: {}", e)))?;

                let wasm_list = wasm_binaries.wasm_binaries()
                    .ok_or_else(|| fff_core::errors::Error::General("WASM binaries list is empty".to_string()))?;

                let mut wasms = HashMap::new();
                for (id, loc) in wasm_list.iter().enumerate() {
                    let mut buf: Vec<u8> = vec![0; loc.size_() as usize];
                    read.read_exact_at(&mut buf, loc.offset())?;
                    let wasm_id = WASMId(id as u32);

                    let start = std::time::Instant::now();
                    debug!(wasm_id = id, size = loc.size_(), offset = loc.offset(), "Creating WASM runtime");

                    let rt = Arc::new(Runtime::try_new(&buf)
                        .map_err(|e| fff_core::errors::Error::General(format!("Failed to create WASM runtime for id {}: {}", id, e)))?);

                    let elapsed = start.elapsed();
                    info!(wasm_id = id, creation_time_ms = elapsed.as_millis(), "WASM runtime created successfully");

                    wasms.insert(wasm_id, rt);
                }
                info!(num_runtimes = wasms.len(), "All WASM runtimes initialized");
                Ok(wasms)
            };

            init()
        });

        // Handle the Result from initialization
        let runtimes = runtimes_result.as_ref()
            .map_err(|e| fff_core::errors::Error::General(format!("WASM initialization failed: {}", e)))?;

        runtimes.get(&wasm_id)
            .cloned()
            .ok_or_else(|| fff_core::errors::Error::General(format!("WASM runtime not found for id: {}", wasm_id.0)))
    }

    pub fn get_encoding_versions(&self) -> Option<&HashMap<fb::EncodingType, Version>> {
        self.encoding_versions.as_ref()
    }
}

//! Lazy, verified, read-only model cache. No network I/O or runtime downloads.
//! EN/RU are bundled. Other languages use existing verified local model files;
//! missing models retain the conversion planner's existing fallback.

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        mpsc::{self, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

use fst::Map;
use sha2::{Digest, Sha256};

use super::model_catalog::{MAX_MODEL_BYTES, MODELS, ModelSpec};

pub(super) type ProbabilityModel = Map<Cow<'static, [u8]>>;
const RETRY_DELAY: Duration = Duration::from_secs(5 * 60);

#[derive(Default)]
struct CacheState {
    ready: HashMap<&'static str, Arc<ProbabilityModel>>,
    pending: HashSet<&'static str>,
    failed: HashMap<&'static str, Instant>,
}

impl CacheState {
    fn request(&mut self, code: &'static str, now: Instant) -> bool {
        if self.ready.contains_key(code)
            || self.pending.contains(code)
            || self
                .failed
                .get(code)
                .is_some_and(|failed| now.duration_since(*failed) < RETRY_DELAY)
        {
            return false;
        }
        self.pending.insert(code);
        true
    }
}

fn state() -> &'static Mutex<CacheState> {
    static STATE: OnceLock<Mutex<CacheState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(CacheState::default()))
}

pub(super) fn model(spec: &'static ModelSpec) -> Option<Arc<ProbabilityModel>> {
    {
        let mut state = state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(model) = state.ready.get(spec.code) {
            return Some(Arc::clone(model));
        }
        if let Some(bytes) = embedded_bytes(spec) {
            let model = Arc::new(Map::new(Cow::Borrowed(bytes)).ok()?);
            state.ready.insert(spec.code, Arc::clone(&model));
            return Some(model);
        }
        if !state.request(spec.code, Instant::now()) {
            return None;
        }
    }

    if cache_loader().is_none_or(|sender| sender.try_send(spec).is_err()) {
        let mut state = state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.pending.remove(spec.code);
        state.failed.insert(spec.code, Instant::now());
    }
    None
}

fn cache_loader() -> Option<&'static SyncSender<&'static ModelSpec>> {
    static SENDER: OnceLock<Option<SyncSender<&'static ModelSpec>>> = OnceLock::new();
    SENDER
        .get_or_init(|| {
            let (sender, receiver) = mpsc::sync_channel::<&'static ModelSpec>(MODELS.len());
            let spawned = thread::Builder::new()
                .name("layout-models".to_owned())
                .spawn(move || {
                    while let Ok(spec) = receiver.recv() {
                        let result = load_external_model(spec);
                        let mut state = state()
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        state.pending.remove(spec.code);
                        match result {
                            Ok(model) => {
                                state.ready.insert(spec.code, Arc::new(model));
                                state.failed.remove(spec.code);
                                tracing::debug!(language = spec.code, "layout model is ready");
                            }
                            Err(error) => {
                                state.failed.insert(spec.code, Instant::now());
                                tracing::debug!(
                                    language = spec.code,
                                    error = %error,
                                    "layout model unavailable; retaining deterministic fallback"
                                );
                            }
                        }
                    }
                });
            spawned.ok().map(|_| sender)
        })
        .as_ref()
}

fn cache_file(spec: &ModelSpec) -> io::Result<PathBuf> {
    let root = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOCALAPPDATA is unavailable"))?;
    Ok(root
        .join("RustSwitcher")
        .join("models")
        .join(spec.sha256)
        .join(format!("{}.fst", spec.code)))
}

fn load_external_model(spec: &ModelSpec) -> io::Result<ProbabilityModel> {
    load_cached_model(&cache_file(spec)?, spec)
}

fn load_cached_model(path: &Path, spec: &ModelSpec) -> io::Result<ProbabilityModel> {
    let bytes = read_verified(path, spec)?;
    Map::new(Cow::Owned(bytes))
        .map_err(|_| invalid_data("model is not a valid FST probability map"))
}

fn read_verified(path: &Path, spec: &ModelSpec) -> io::Result<Vec<u8>> {
    validate_size(spec)?;
    let file = File::open(path)?;
    if file.metadata()?.len() != spec.byte_len as u64 {
        return Err(invalid_data(
            "cached model length does not match the manifest",
        ));
    }
    let mut bytes = Vec::with_capacity(spec.byte_len);
    file.take(spec.byte_len as u64 + 1)
        .read_to_end(&mut bytes)?;
    verify_payload(spec, &bytes)?;
    Ok(bytes)
}

fn validate_size(spec: &ModelSpec) -> io::Result<()> {
    if spec.byte_len == 0 || spec.byte_len > MAX_MODEL_BYTES {
        return Err(invalid_data("model length exceeds the configured limit"));
    }
    Ok(())
}

fn verify_payload(spec: &ModelSpec, bytes: &[u8]) -> io::Result<()> {
    validate_size(spec)?;
    if bytes.len() != spec.byte_len {
        return Err(invalid_data("model length does not match the manifest"));
    }
    if format!("{:x}", Sha256::digest(bytes)) != spec.sha256 {
        return Err(invalid_data("model SHA-256 does not match the manifest"));
    }
    Ok(())
}

fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(not(test))]
fn embedded_bytes(spec: &ModelSpec) -> Option<&'static [u8]> {
    let directory = match spec.code {
        "en" => lingua_english_language_model::ENGLISH_MODELS_DIRECTORY,
        "ru" => lingua_russian_language_model::RUSSIAN_MODELS_DIRECTORY,
        _ => return None,
    };
    directory.get_file("ngrams.fst").map(|file| file.contents())
}

#[cfg(test)]
fn embedded_bytes(spec: &ModelSpec) -> Option<&'static [u8]> {
    let directory = match spec.code {
        "af" => lingua_afrikaans_language_model::AFRIKAANS_MODELS_DIRECTORY,
        "sq" => lingua_albanian_language_model::ALBANIAN_MODELS_DIRECTORY,
        "ar" => lingua_arabic_language_model::ARABIC_MODELS_DIRECTORY,
        "hy" => lingua_armenian_language_model::ARMENIAN_MODELS_DIRECTORY,
        "az" => lingua_azerbaijani_language_model::AZERBAIJANI_MODELS_DIRECTORY,
        "eu" => lingua_basque_language_model::BASQUE_MODELS_DIRECTORY,
        "be" => lingua_belarusian_language_model::BELARUSIAN_MODELS_DIRECTORY,
        "bn" => lingua_bengali_language_model::BENGALI_MODELS_DIRECTORY,
        "nb" => lingua_bokmal_language_model::BOKMAL_MODELS_DIRECTORY,
        "bs" => lingua_bosnian_language_model::BOSNIAN_MODELS_DIRECTORY,
        "bg" => lingua_bulgarian_language_model::BULGARIAN_MODELS_DIRECTORY,
        "ca" => lingua_catalan_language_model::CATALAN_MODELS_DIRECTORY,
        "zh" => lingua_chinese_language_model::CHINESE_MODELS_DIRECTORY,
        "hr" => lingua_croatian_language_model::CROATIAN_MODELS_DIRECTORY,
        "cs" => lingua_czech_language_model::CZECH_MODELS_DIRECTORY,
        "da" => lingua_danish_language_model::DANISH_MODELS_DIRECTORY,
        "nl" => lingua_dutch_language_model::DUTCH_MODELS_DIRECTORY,
        "en" => lingua_english_language_model::ENGLISH_MODELS_DIRECTORY,
        "eo" => lingua_esperanto_language_model::ESPERANTO_MODELS_DIRECTORY,
        "et" => lingua_estonian_language_model::ESTONIAN_MODELS_DIRECTORY,
        "fi" => lingua_finnish_language_model::FINNISH_MODELS_DIRECTORY,
        "fr" => lingua_french_language_model::FRENCH_MODELS_DIRECTORY,
        "lg" => lingua_ganda_language_model::GANDA_MODELS_DIRECTORY,
        "ka" => lingua_georgian_language_model::GEORGIAN_MODELS_DIRECTORY,
        "de" => lingua_german_language_model::GERMAN_MODELS_DIRECTORY,
        "el" => lingua_greek_language_model::GREEK_MODELS_DIRECTORY,
        "gu" => lingua_gujarati_language_model::GUJARATI_MODELS_DIRECTORY,
        "he" => lingua_hebrew_language_model::HEBREW_MODELS_DIRECTORY,
        "hi" => lingua_hindi_language_model::HINDI_MODELS_DIRECTORY,
        "hu" => lingua_hungarian_language_model::HUNGARIAN_MODELS_DIRECTORY,
        "is" => lingua_icelandic_language_model::ICELANDIC_MODELS_DIRECTORY,
        "id" => lingua_indonesian_language_model::INDONESIAN_MODELS_DIRECTORY,
        "ga" => lingua_irish_language_model::IRISH_MODELS_DIRECTORY,
        "it" => lingua_italian_language_model::ITALIAN_MODELS_DIRECTORY,
        "ja" => lingua_japanese_language_model::JAPANESE_MODELS_DIRECTORY,
        "kk" => lingua_kazakh_language_model::KAZAKH_MODELS_DIRECTORY,
        "ko" => lingua_korean_language_model::KOREAN_MODELS_DIRECTORY,
        "la" => lingua_latin_language_model::LATIN_MODELS_DIRECTORY,
        "lv" => lingua_latvian_language_model::LATVIAN_MODELS_DIRECTORY,
        "lt" => lingua_lithuanian_language_model::LITHUANIAN_MODELS_DIRECTORY,
        "mk" => lingua_macedonian_language_model::MACEDONIAN_MODELS_DIRECTORY,
        "ms" => lingua_malay_language_model::MALAY_MODELS_DIRECTORY,
        "mi" => lingua_maori_language_model::MAORI_MODELS_DIRECTORY,
        "mr" => lingua_marathi_language_model::MARATHI_MODELS_DIRECTORY,
        "mn" => lingua_mongolian_language_model::MONGOLIAN_MODELS_DIRECTORY,
        "nn" => lingua_nynorsk_language_model::NYNORSK_MODELS_DIRECTORY,
        "fa" => lingua_persian_language_model::PERSIAN_MODELS_DIRECTORY,
        "pl" => lingua_polish_language_model::POLISH_MODELS_DIRECTORY,
        "pt" => lingua_portuguese_language_model::PORTUGUESE_MODELS_DIRECTORY,
        "pa" => lingua_punjabi_language_model::PUNJABI_MODELS_DIRECTORY,
        "ro" => lingua_romanian_language_model::ROMANIAN_MODELS_DIRECTORY,
        "ru" => lingua_russian_language_model::RUSSIAN_MODELS_DIRECTORY,
        "sr" => lingua_serbian_language_model::SERBIAN_MODELS_DIRECTORY,
        "sn" => lingua_shona_language_model::SHONA_MODELS_DIRECTORY,
        "sk" => lingua_slovak_language_model::SLOVAK_MODELS_DIRECTORY,
        "sl" => lingua_slovene_language_model::SLOVENE_MODELS_DIRECTORY,
        "so" => lingua_somali_language_model::SOMALI_MODELS_DIRECTORY,
        "st" => lingua_sotho_language_model::SOTHO_MODELS_DIRECTORY,
        "es" => lingua_spanish_language_model::SPANISH_MODELS_DIRECTORY,
        "sw" => lingua_swahili_language_model::SWAHILI_MODELS_DIRECTORY,
        "sv" => lingua_swedish_language_model::SWEDISH_MODELS_DIRECTORY,
        "tl" => lingua_tagalog_language_model::TAGALOG_MODELS_DIRECTORY,
        "ta" => lingua_tamil_language_model::TAMIL_MODELS_DIRECTORY,
        "te" => lingua_telugu_language_model::TELUGU_MODELS_DIRECTORY,
        "th" => lingua_thai_language_model::THAI_MODELS_DIRECTORY,
        "ts" => lingua_tsonga_language_model::TSONGA_MODELS_DIRECTORY,
        "tn" => lingua_tswana_language_model::TSWANA_MODELS_DIRECTORY,
        "tr" => lingua_turkish_language_model::TURKISH_MODELS_DIRECTORY,
        "uk" => lingua_ukrainian_language_model::UKRAINIAN_MODELS_DIRECTORY,
        "ur" => lingua_urdu_language_model::URDU_MODELS_DIRECTORY,
        "vi" => lingua_vietnamese_language_model::VIETNAMESE_MODELS_DIRECTORY,
        "cy" => lingua_welsh_language_model::WELSH_MODELS_DIRECTORY,
        "xh" => lingua_xhosa_language_model::XHOSA_MODELS_DIRECTORY,
        "yo" => lingua_yoruba_language_model::YORUBA_MODELS_DIRECTORY,
        "zu" => lingua_zulu_language_model::ZULU_MODELS_DIRECTORY,
        _ => return None,
    };
    directory.get_file("ngrams.fst").map(|file| file.contents())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{super::model_catalog::language_for_tag, *};

    fn english() -> &'static ModelSpec {
        language_for_tag("en-US").expect("English manifest missing")
    }

    fn english_bytes() -> &'static [u8] {
        embedded_bytes(english()).expect("English test fixture missing")
    }

    #[test]
    fn every_manifest_matches_its_published_model_crate() {
        assert_eq!(MODELS.len(), 75);
        let mut codes = HashSet::new();
        for spec in MODELS {
            assert!(codes.insert(spec.code), "duplicate {}", spec.code);
            assert_eq!(spec.code.len(), 2);
            assert!(
                spec.code
                    .bytes()
                    .all(|character| character.is_ascii_lowercase())
            );
            assert_eq!(spec.iso_639_3.len(), 3);
            assert_eq!(spec.sha256.len(), 64);
            let bytes = embedded_bytes(spec).expect("model fixture missing");
            assert!(verify_payload(spec, bytes).is_ok(), "{}", spec.code);
            assert!(Map::new(bytes).is_ok(), "{}", spec.code);
        }
    }

    #[test]
    fn verified_cached_model_remains_available_offline() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("en.fst");
        fs::write(&path, english_bytes())?;
        let loaded = load_cached_model(&path, english())?;
        assert!(loaded.contains_key("hello"));
        assert_eq!(fs::read(&path)?, english_bytes());
        Ok(())
    }

    #[test]
    fn missing_model_is_not_created() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("missing.fst");
        assert!(
            matches!(load_cached_model(&path, english()), Err(error) if error.kind() == io::ErrorKind::NotFound)
        );
        assert!(!path.exists());
        assert_eq!(fs::read_dir(directory.path())?.count(), 0);
        Ok(())
    }

    #[test]
    fn corrupt_and_truncated_cache_are_rejected_without_modification() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("en.fst");
        let mut corrupt = english_bytes().to_vec();
        corrupt[0] ^= 0xFF;
        fs::write(&path, &corrupt)?;
        assert!(load_cached_model(&path, english()).is_err());
        assert_eq!(fs::read(&path)?, corrupt);
        fs::write(&path, [0; 16])?;
        assert!(load_cached_model(&path, english()).is_err());
        assert_eq!(fs::read(&path)?, [0; 16]);
        Ok(())
    }

    #[test]
    fn cache_requests_are_deduplicated_and_failed_reads_have_a_cooldown() {
        let now = Instant::now();
        let mut state = CacheState::default();
        assert!(state.request("de", now));
        assert!(!state.request("de", now));
        state.pending.remove("de");
        state.failed.insert("de", now);
        assert!(!state.request("de", now + Duration::from_secs(1)));
        assert!(state.request("de", now + RETRY_DELAY));
    }

    #[test]
    fn manifest_length_limit_is_enforced_before_allocation() {
        for byte_len in [0, MAX_MODEL_BYTES + 1] {
            let invalid = ModelSpec {
                byte_len,
                ..*english()
            };
            assert!(verify_payload(&invalid, &[]).is_err());
        }
    }
}

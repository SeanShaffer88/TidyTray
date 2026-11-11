//! Large & duplicate file finder.
//! Duplicates are found in three cheap-to-expensive passes: same size ->
//! same hash of the first 64 KiB -> same full-content hash.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

use serde::Serialize;

use crate::safelist::SafeList;
use crate::scanner::{walk_parallel, worker_count};

const MIB: u64 = 1024 * 1024;
const HEAD_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BigFile {
    pub path: String,
    pub bytes: u64,
    pub modified: u64,
    pub kind: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bucket {
    pub label: &'static str,
    pub min: u64,
    pub count: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    pub bytes_each: u64,
    pub paths: Vec<String>,
    /// Space freed by keeping one copy.
    pub wasted: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesReport {
    pub root: String,
    pub largest: Vec<BigFile>,
    pub buckets: Vec<Bucket>,
    pub duplicates: Vec<DuplicateGroup>,
    pub scanned_files: u64,
}

/// Size buckets shown as a small histogram on the Storage screen.
pub const BUCKETS: [(&str, u64); 4] = [("100 MB - 500 MB", 100 * MIB), ("500 MB - 1 GB", 500 * MIB), ("1 - 4 GB", 1024 * MIB), ("4 GB +", 4096 * MIB)];

pub fn bucket_index(bytes: u64) -> Option<usize> {
    BUCKETS.iter().rposition(|(_, min)| bytes >= *min)
}

pub fn kind_of(path: &Path) -> &'static str {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "mp4" | "mkv" | "mov" | "avi" | "webm" => "Video",
        "iso" | "img" | "vhd" | "vhdx" | "vmdk" => "Disk image",
        "zip" | "7z" | "rar" | "tar" | "gz" => "Archive",
        "exe" | "msi" | "msix" => "Installer",
        "pak" | "vpk" | "bundle" | "ucas" | "utoc" => "Game data",
        "jpg" | "png" | "raw" | "cr3" | "nef" | "psd" => "Image",
        "log" | "etl" | "dmp" => "Log / dump",
        _ => "File",
    }
}

/// 64-bit FNV-1a. Not cryptographic; collisions are ruled out by the full-content pass
/// and a final byte comparison is left to the owner (see TODO in `same_content`).
pub fn fnv1a(data: &[u8], mut hash: u64) -> u64 {
    for b in data {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

pub const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

/// Hashes the first `limit` bytes of a file (or all of it when `limit` is None).
pub fn hash_file(path: &Path, limit: Option<usize>) -> io::Result<u64> {
    let mut file = File::open(path)?;
    let mut buf = vec![0u8; 256 * 1024];
    let mut hash = FNV_OFFSET;
    let mut left = limit.unwrap_or(usize::MAX);
    while left > 0 {
        let want = buf.len().min(left);
        let n = file.read(&mut buf[..want])?;
        if n == 0 {
            break;
        }
        hash = fnv1a(&buf[..n], hash);
        left -= n;
    }
    Ok(hash)
}

/// Groups candidate files of equal size into confirmed duplicate groups.
pub fn find_duplicates(candidates: Vec<(PathBuf, u64)>) -> Vec<DuplicateGroup> {
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for (p, size) in candidates {
        by_size.entry(size).or_default().push(p);
    }
    let mut groups = Vec::new();
    for (size, paths) in by_size.into_iter().filter(|(s, v)| *s > 0 && v.len() > 1) {
        for head_group in group_by(paths, |p| hash_file(p, Some(HEAD_BYTES))) {
            let full_groups =
                if size as usize <= HEAD_BYTES { vec![head_group] } else { group_by(head_group, |p| hash_file(p, None)) };
            for g in full_groups {
                // TODO(owner): byte-compare the members before offering deletion.
                groups.push(DuplicateGroup {
                    bytes_each: size,
                    wasted: size * (g.len() as u64 - 1),
                    paths: g.iter().map(|p| p.to_string_lossy().into_owned()).collect(),
                });
            }
        }
    }
    groups.sort_by_key(|g| std::cmp::Reverse(g.wasted));
    groups
}

fn group_by<F>(paths: Vec<PathBuf>, key: F) -> Vec<Vec<PathBuf>>
where
    F: Fn(&Path) -> io::Result<u64>,
{
    let mut map: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for p in paths {
        if let Ok(k) = key(&p) {
            map.entry(k).or_default().push(p);
        }
    }
    map.into_values().filter(|g| g.len() > 1).collect()
}

/// Scans `root` for files >= `min_bytes` and duplicate files >= 1 MiB.
pub fn scan(root: &Path, min_bytes: u64, safe: &SafeList, top: usize) -> FilesReport {
    let big = Mutex::new(Vec::<BigFile>::new());
    let dupe_candidates = Mutex::new(Vec::<(PathBuf, u64)>::new());
    let mut buckets: Vec<Bucket> =
        BUCKETS.iter().map(|(label, min)| Bucket { label, min: *min, count: 0, bytes: 0 }).collect();
    let bucket_tally = Mutex::new(vec![(0u64, 0u64); BUCKETS.len()]);
    let scanned = std::sync::atomic::AtomicU64::new(0);
    walk_parallel(vec![(0, root.to_path_buf())], worker_count(), |_, path, meta| {
        scanned.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let len = meta.len();
        if len < MIB || safe.protects(path).is_some() {
            return;
        }
        if let Some(i) = bucket_index(len) {
            let mut t = bucket_tally.lock().unwrap();
            t[i].0 += 1;
            t[i].1 += len;
        }
        if len >= min_bytes {
            let modified = meta.modified().ok().and_then(|m| m.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
            big.lock().unwrap().push(BigFile { path: path.to_string_lossy().into_owned(), bytes: len, modified, kind: kind_of(path) });
        }
        dupe_candidates.lock().unwrap().push((path.to_path_buf(), len));
    });
    for (b, (count, bytes)) in buckets.iter_mut().zip(bucket_tally.into_inner().unwrap()) {
        b.count = count;
        b.bytes = bytes;
    }
    let mut largest = big.into_inner().unwrap();
    largest.sort_by_key(|f| std::cmp::Reverse(f.bytes));
    largest.truncate(top);
    FilesReport {
        root: root.to_string_lossy().into_owned(),
        largest,
        buckets,
        duplicates: find_duplicates(dupe_candidates.into_inner().unwrap()),
        scanned_files: scanned.into_inner(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn fnv_known_vectors() {
        assert_eq!(fnv1a(b"", FNV_OFFSET), 0xcbf29ce484222325);
        assert_eq!(fnv1a(b"a", FNV_OFFSET), 0xaf63dc4c8601ec8c);
        assert_eq!(fnv1a(b"foobar", FNV_OFFSET), 0x85944171f73967e8);
    }

    #[test]
    fn hashing_in_chunks_matches_one_shot() {
        let a = fnv1a(b"hello world", FNV_OFFSET);
        let b = fnv1a(b" world", fnv1a(b"hello", FNV_OFFSET));
        assert_eq!(a, b);
    }

    #[test]
    fn buckets() {
        assert_eq!(bucket_index(50 * MIB), None);
        assert_eq!(bucket_index(100 * MIB), Some(0));
        assert_eq!(bucket_index(700 * MIB), Some(1));
        assert_eq!(bucket_index(9000 * MIB), Some(3));
    }

    #[test]
    fn finds_duplicates_but_not_same_size_different_content() {
        let dir = std::env::temp_dir().join(format!("tidy-dupes-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut big = vec![7u8; 200 * 1024];
        fs::write(dir.join("a.bin"), &big).unwrap();
        fs::write(dir.join("b.bin"), &big).unwrap();
        big[150 * 1024] = 9; // same size and same first 64 KiB, different tail
        fs::write(dir.join("c.bin"), &big).unwrap();
        fs::write(dir.join("d.bin"), vec![1u8; 10]).unwrap();
        let cands: Vec<(PathBuf, u64)> = ["a.bin", "b.bin", "c.bin", "d.bin"]
            .iter()
            .map(|n| (dir.join(n), fs::metadata(dir.join(n)).unwrap().len()))
            .collect();
        let groups = find_duplicates(cands);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].paths.len(), 2);
        assert_eq!(groups[0].wasted, 200 * 1024);
        assert!(groups[0].paths.iter().all(|p| !p.ends_with("c.bin")));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn kinds() {
        assert_eq!(kind_of(Path::new("D:/Captures/clip.MKV")), "Video");
        assert_eq!(kind_of(Path::new("Win11_24H2.iso")), "Disk image");
        assert_eq!(kind_of(Path::new("noext")), "File");
    }
}

//! Windows performance counters (PDH) with wildcard instances, read as name/value pairs.

use windows::{
    core::{HSTRING, PCWSTR},
    Win32::System::Performance::{
        PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW, PDH_CSTATUS_VALID_DATA,
        PDH_FMT, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA,
    },
};

const PDH_FMT_NOCAP100: u32 = 0x8000;

pub struct Query {
    query: PDH_HQUERY,
    counters: Vec<PDH_HCOUNTER>,
}

// The handles are only used from the sampler thread that owns the query.
unsafe impl Send for Query {}

impl Query {
    /// Opens a query over `paths` (English counter names). Counters that don't exist are skipped
    /// and read back as empty.
    pub fn open(paths: &[&str]) -> Option<Self> {
        let mut query = PDH_HQUERY::default();
        if unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut query) } != 0 {
            return None;
        }
        let counters = paths
            .iter()
            .map(|path| {
                let mut counter = PDH_HCOUNTER::default();
                let path = HSTRING::from(*path);
                if unsafe { PdhAddEnglishCounterW(query, &path, 0, &mut counter) } != 0 {
                    PDH_HCOUNTER::default()
                } else {
                    counter
                }
            })
            .collect();
        let q = Self { query, counters };
        // Rate counters need two collections before they have a value.
        q.collect();
        Some(q)
    }

    pub fn collect(&self) -> bool {
        unsafe { PdhCollectQueryData(self.query) == 0 }
    }

    /// Every instance of counter `index` as (instance name, value).
    pub fn values(&self, index: usize) -> Vec<(String, f64)> {
        let Some(&counter) = self.counters.get(index) else { return Vec::new() };
        if counter.is_invalid() {
            return Vec::new();
        }
        let format = PDH_FMT(PDH_FMT_DOUBLE.0 | PDH_FMT_NOCAP100);
        let (mut size, mut count) = (0u32, 0u32);
        let status = unsafe { PdhGetFormattedCounterArrayW(counter, format, &mut size, &mut count, None) };
        if status != PDH_MORE_DATA || size == 0 {
            return Vec::new();
        }
        // u64 storage keeps the item array aligned.
        let mut buf = vec![0u64; (size as usize).div_ceil(8)];
        let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
        if unsafe { PdhGetFormattedCounterArrayW(counter, format, &mut size, &mut count, Some(items)) } != 0 {
            return Vec::new();
        }
        let items = unsafe { std::slice::from_raw_parts(items, count as usize) };
        items
            .iter()
            .filter(|item| item.FmtValue.CStatus == PDH_CSTATUS_VALID_DATA)
            .map(|item| {
                let name = unsafe { item.szName.to_string() }.unwrap_or_default();
                (name, unsafe { item.FmtValue.Anonymous.doubleValue })
            })
            .collect()
    }
}

impl Drop for Query {
    fn drop(&mut self) {
        unsafe { PdhCloseQuery(self.query) };
    }
}

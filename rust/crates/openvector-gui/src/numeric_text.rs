use std::{borrow::Cow, sync::OnceLock};

/// Single-character numeric symbols. A NUL group means no grouping separator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NumericLocale {
    pub decimal: char, pub group: char, pub zero: char,
    pub positive: char, pub negative: char, pub exponent: char,
}
impl Default for NumericLocale {
    fn default() -> Self { Self { decimal: '.', group: ',', zero: '0', positive: '+', negative: '-', exponent: 'e' } }
}
fn space(character: char) -> bool {
    matches!(character as u32, 9..=13 | 32 | 160 | 5760 | 6158 | 8192..=8202 | 8232 | 8233 | 8239 | 8287 | 12288)
}
impl NumericLocale {
    /// Capture the host's numeric symbols on first use, as the native text boundary does.
    /// Discovery never changes the process C locale. Errors remain explicit for the session.
    pub fn host() -> Result<Self, String> {
        static HOST: OnceLock<Result<NumericLocale, String>> = OnceLock::new();
        HOST.get_or_init(host_numeric_locale).clone()
    }
    fn normalize<'a>(&self, text: &'a str) -> Option<Cow<'a, str>> {
        let text = text.trim_start_matches(space);
        // POSIX C has no grouping symbol. Both it and the native C comma marker
        // reject grouped input, so neither needs an allocated ASCII normalization.
        let ascii = Self { group: ',', ..*self } == Self::default() && matches!(self.group, ',' | '\0');
        if ascii && text.is_ascii() { return Some(Cow::Borrowed(text.trim_end_matches(space))); }
        let mut token = String::with_capacity(text.len());
        let mut characters = text.chars();
        while let Some(character) = characters.next() {
            let code = character as u32;
            let zero = self.zero as u32;
            let mapped = if (zero..zero + 10).contains(&code) { char::from(b'0' + (code - zero) as u8) }
                else if character.is_ascii_digit() { character }
                else if character == self.positive { '+' }
                else if character == self.negative { '-' }
                else if character == self.decimal { '.' }
                else if character == self.group || self.group == '\u{a0}' && character == ' ' { ',' }
                else if character == self.exponent || self.exponent.to_uppercase().eq(std::iter::once(character)) { 'e' }
                else if character.is_ascii_alphabetic() { character.to_ascii_lowercase() }
                else { if !space(character) || characters.any(|character| !space(character)) { return None; } break; };
            token.push(mapped);
        }
        Some(Cow::Owned(token))
    }
    pub fn integer(&self, text: &str) -> Option<i32> {
        for locale in [*self, Self::default()] {
            if let Some(token) = locale.normalize(text) {
                let digits = token.strip_prefix(['+', '-']).unwrap_or(&token);
                if !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()) {
                    if let Ok(value) = token.parse::<i32>() { return Some(value); }
                }
            }
        }
        None
    }
    pub fn double(&self, text: &str) -> Option<f64> {
        for locale in [*self, Self::default()] { if let Some(token) = locale.normalize(text) { if let Some(value) = double_token(&token) { return Some(value); } } }
        None
    }
    pub fn percent(&self, text: &str) -> Option<f64> { self.double(text.strip_suffix('%').unwrap_or(text)).map(|value| value * 0.01) }
}

#[cfg(target_os = "linux")]
fn host_numeric_locale() -> Result<NumericLocale, String> {
    // An empty name resolves LC_ALL, then LC_NUMERIC, then LANG. A private locale
    // object reads the environment even when Rust/GPUI leave libc's global locale at C.
    posix_numeric_locale(c"")
}

#[cfg(target_os = "linux")]
fn posix_numeric_locale(name: &std::ffi::CStr) -> Result<NumericLocale, String> {
    use std::{ffi::CStr, ptr};
    // SAFETY: name is NUL-terminated and a null base requests a new owned locale.
    let locale = unsafe { libc::newlocale(libc::LC_NUMERIC_MASK, name.as_ptr(), ptr::null_mut()) };
    if locale.is_null() {
        return Err(format!("Cannot load the numeric locale: {}. Check LC_ALL, LC_NUMERIC and LANG, install the selected locale, then restart OpenVector.", std::io::Error::last_os_error()));
    }
    let read = |item, label, optional| -> Result<char, String> {
        // SAFETY: locale is live and both callers supply valid LC_NUMERIC items.
        let text = unsafe { CStr::from_ptr(libc::nl_langinfo_l(item, locale)) }.to_str()
            .map_err(|_| format!("The host {label} is not UTF-8. Select a UTF-8 numeric locale and restart OpenVector."))?;
        let mut characters = text.chars();
        match (characters.next(), characters.next()) {
            (Some(character), None) => Ok(character),
            (None, None) if optional => Ok('\0'), // No grouping symbol, as in POSIX C.
            _ => Err(format!("The host {label} must be one character.")),
        }
    };
    // POSIX LC_NUMERIC defines only these separators. Its numeric syntax uses
    // ASCII digits/signs/exponents; no language-to-symbol lookup is inferred.
    let symbols = (|| Ok(NumericLocale { decimal: read(libc::RADIXCHAR, "decimal separator", false)?,
        group: read(libc::THOUSEP, "group separator", true)?, ..NumericLocale::default() }))();
    // SAFETY: locale is owned here; both strings were consumed before freeing it,
    // including when a read failed. No thread or global locale ever points to it.
    unsafe { libc::freelocale(locale); }
    symbols
}

#[cfg(target_os = "windows")]
fn host_numeric_locale() -> Result<NumericLocale, String> {
    use windows_sys::Win32::Globalization::{GetLocaleInfoW, GetThreadLocale, LOCALE_SDECIMAL,
        LOCALE_STHOUSAND, LOCALE_SNATIVEDIGITS, LOCALE_SNEGATIVESIGN};
    // Match the retained Qt 4.4.3 Windows boundary, including user overrides and
    // its first-QChar handling of the ten-digit list and negative sign.
    let locale = unsafe { GetThreadLocale() };
    let read = |kind, label, optional| -> Result<char, String> {
        let mut buffer = [0u16; 32];
        // SAFETY: buffer is writable for the supplied number of UTF-16 code units.
        let count = unsafe { GetLocaleInfoW(locale, kind, buffer.as_mut_ptr(), buffer.len() as i32) };
        if count == 0 { return Err(format!("Cannot read the host {label}: {}.", std::io::Error::last_os_error())); }
        if count == 1 {
            return if optional { Ok('\0') } else { Err(format!("The host {label} is empty.")) };
        }
        char::from_u32(buffer[0] as u32).ok_or_else(|| format!("The host {label} is not a single Unicode character."))
    };
    Ok(NumericLocale { decimal: read(LOCALE_SDECIMAL, "decimal separator", false)?,
        group: read(LOCALE_STHOUSAND, "group separator", true)?,
        zero: read(LOCALE_SNATIVEDIGITS, "zero digit", false)?,
        negative: read(LOCALE_SNEGATIVESIGN, "negative sign", false)?, ..NumericLocale::default() })
}

#[cfg(target_os = "macos")]
fn host_numeric_locale() -> Result<NumericLocale, String> {
    use core_foundation_sys::{base::{CFRelease, kCFAllocatorDefault}, locale::CFLocaleCopyCurrent,
        number::kCFNumberSInt32Type, number_formatter::*, string::{CFStringGetCharacterAtIndex, CFStringGetLength, CFStringRef}};
    // Core Foundation includes the user's number-format preferences even when a
    // Finder-launched process has no POSIX locale environment.
    let locale = unsafe { CFLocaleCopyCurrent() };
    if locale.is_null() { return Err("Cannot read the macOS numeric locale.".into()); }
    // SAFETY: locale is retained through formatter creation, which retains it.
    let formatter = unsafe { CFNumberFormatterCreate(kCFAllocatorDefault, locale, kCFNumberFormatterDecimalStyle) };
    unsafe { CFRelease(locale.cast()); }
    if formatter.is_null() { return Err("Cannot create the macOS numeric formatter.".into()); }
    let character = |value: CFStringRef, label, optional| -> Result<char, String> {
        if value.is_null() { return Err(format!("Cannot read the host {label}.")); }
        // SAFETY: these Copy/Create functions return owned CFStrings for the
        // documented string properties and formatted zero. Release on every path.
        let symbol = unsafe { match CFStringGetLength(value) {
            0 if optional => Ok('\0'),
            1 => char::from_u32(CFStringGetCharacterAtIndex(value, 0) as u32)
                .ok_or_else(|| format!("The host {label} is not a single Unicode character.")),
            _ => Err(format!("The host {label} must be one character.")),
        } };
        unsafe { CFRelease(value.cast()); }
        symbol
    };
    let read = |key, label, optional| character(unsafe { CFNumberFormatterCopyProperty(formatter, key) }.cast(), label, optional);
    let symbols = (|| {
        let zero = 0i32;
        let zero_text = unsafe { CFNumberFormatterCreateStringWithValue(kCFAllocatorDefault, formatter,
            kCFNumberSInt32Type, (&zero as *const i32).cast()) };
        Ok(NumericLocale { zero: character(zero_text, "zero digit", false)?,
            decimal: read(unsafe { kCFNumberFormatterDecimalSeparator }, "decimal separator", false)?,
            group: read(unsafe { kCFNumberFormatterGroupingSeparator }, "group separator", true)?,
            positive: read(unsafe { kCFNumberFormatterPlusSign }, "positive sign", false)?,
            negative: read(unsafe { kCFNumberFormatterMinusSign }, "negative sign", false)?,
            exponent: read(unsafe { kCFNumberFormatterExponentSymbol }, "exponent symbol", false)? })
    })();
    // SAFETY: every copied property is released and no parser references formatter.
    unsafe { CFRelease(formatter.cast()); }
    symbols
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn host_numeric_locale() -> Result<NumericLocale, String> {
    Err("Host numeric locale discovery is not supported on this platform.".into())
}

fn double_token(token: &str) -> Option<f64> {
    if token.eq_ignore_ascii_case("nan") { return Some(f64::NAN); }
    if token.eq_ignore_ascii_case("inf") || token.eq_ignore_ascii_case("+inf") { return Some(f64::INFINITY); }
    if token.eq_ignore_ascii_case("-inf") { return Some(f64::NEG_INFINITY); }
    let bytes = token.as_bytes();
    let mut index = usize::from(bytes.first().is_some_and(|byte| matches!(byte, b'+' | b'-')));
    let mut digits = 0;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) { index += 1; digits += 1; }
    if bytes.get(index) == Some(&b'.') { index += 1; while bytes.get(index).is_some_and(u8::is_ascii_digit) { index += 1; digits += 1; } }
    if digits == 0 { return None; }
    let significand_end = index;
    if bytes.get(index).is_some_and(|byte| matches!(byte, b'e' | b'E')) {
        index += 1;
        if bytes.get(index).is_some_and(|byte| matches!(byte, b'+' | b'-')) { index += 1; }
        let exponent_start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) { index += 1; }
        if index == exponent_start { return None; }
    }
    if index != bytes.len() { return None; }
    let value: f64 = token.parse().ok()?;
    if !value.is_finite() || value == 0.0 && bytes[..significand_end].iter().any(|byte| matches!(byte, b'1'..=b'9')) { return None; }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn qt_tokens_keep_native_whitespace_range_and_percent_suffix() {
        let parser = NumericLocale::default();
        assert_eq!(parser.integer("\u{180e}+42\u{a0}"), Some(42));
        assert_eq!(parser.integer("\u{feff}42"), None);
        assert_eq!(parser.integer("2147483648"), None);
        assert_eq!(parser.integer("1,000"), None);
        assert_eq!(parser.percent("125%"), Some(1.25));
        assert_eq!(parser.percent("125% "), None);
        assert_eq!(parser.double("1e-999"), None);
        assert_eq!(parser.double("0e-999"), Some(0.0));
        assert_eq!(parser.double("+nan"), None);
        assert!(parser.double("NAN").unwrap().is_nan());
    }
    #[test]
    fn locale_mapping_precedes_c_fallback_and_space_trimming() {
        let parser = NumericLocale { decimal: ',', group: '\u{a0}', zero: '٠', ..Default::default() };
        assert_eq!(parser.double("١٢,٥"), Some(12.5));
        assert_eq!(parser.double("12.5"), Some(12.5));
        assert_eq!(parser.double("12,5 "), None);
        assert_eq!(parser.integer("٤٢"), Some(42));
    }
    #[test]
    fn spanish_bitmap_percent_keeps_comma_priority_and_c_fallback() {
        let parser = NumericLocale { decimal: ',', group: '.', ..Default::default() };
        assert_eq!(parser.double("1,000"), Some(1.0));
        assert_eq!(parser.double("1.000"), Some(1.0));
        assert_eq!(parser.double("1.234,5"), None);
        assert_eq!(parser.percent("125,0% "), None);
        for text in ["125,0%", "125.0%"] {
            let mut size = crate::bitmap_size::BitmapSize::new(1.0).unwrap();
            size.source_changed(120, 80).unwrap();
            size.commit(0, 2, parser.percent(text).unwrap()).unwrap();
            assert_eq!(size.scale, 1.25, "{text}");
            assert_eq!(size.panes[0], [150, 100], "{text}");
        }
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn posix_discovery_owns_its_locale_and_preserves_global_c_state() {
        use std::ffi::CStr;
        // A null second argument only queries setlocale; it never changes it.
        let before = unsafe { CStr::from_ptr(libc::setlocale(libc::LC_NUMERIC, std::ptr::null())) }.to_owned();
        let parser = posix_numeric_locale(c"C").unwrap();
        assert_eq!(parser.decimal, '.');
        assert_eq!(parser.group, '\0');
        assert_eq!(parser.percent("125.0%"), Some(1.25));
        assert_eq!(parser.percent("125,0%"), None);
        assert!(posix_numeric_locale(c"openvector-missing-locale.invalid").is_err());
        let after = unsafe { CStr::from_ptr(libc::setlocale(libc::LC_NUMERIC, std::ptr::null())) };
        assert_eq!(before.as_c_str(), after);
        assert_eq!(NumericLocale::host().unwrap(), posix_numeric_locale(c"").unwrap());
    }
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "Run in a fresh process with LC_ALL=es_ES.UTF-8 and an installed or private LOCPATH locale."]
    fn host_comma_decimal_bitmap_percent() {
        let parser = NumericLocale::host().unwrap();
        assert_eq!(parser.decimal, ',');
        assert_eq!(parser.group, '.');
        assert_eq!(parser.percent("125,0%"), Some(1.25));
        assert_eq!(parser.percent("125.0%"), Some(1.25));
        assert_eq!(parser.double("1,000"), Some(1.0));
    }
}

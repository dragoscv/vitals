//! A small, generic WMI reader for the hardware inventory.
//!
//! # Why the `windows` bindings rather than the hand-rolled client
//!
//! [`crate::sensors`] carries its own vtable-indexed client because it reads
//! three integers from one class and predates the `Win32_System_Wmi`
//! feature. The inventory reads a dozen classes across two namespaces and
//! half a dozen `VARIANT` shapes; hand-written vtable slots at that breadth
//! are exactly the mis-indexed-method hazard that module documents. The
//! generated bindings make the slot numbers the compiler's problem.
//!
//! # Two pitfalls carried over from the sensors client
//!
//! - `WBEM_FLAG_RETURN_IMMEDIATELY` makes `ExecQuery` succeed before the
//!   provider has run, so a failure surfaces at the first `Next`. An error
//!   there ends the enumeration; it is never mistaken for a row.
//! - `CoSetProxyBlanket` is required. Without it some providers refuse the
//!   call with an access error that looks like a missing class.
//!
//! # Why the CIM type is read alongside the value
//!
//! WMI marshals `uint32` as `VT_I4`, so `AdapterRAM` of `0xFFF00000` arrives
//! as a negative number, and `uint64` as a `BSTR`, so `Capacity` arrives as
//! the text `"51539607552"`. Interpreting the `VARIANT` alone gets both
//! wrong; the `CIMTYPE` that `Get` returns says what the value really is.

use std::collections::HashMap;
use std::marker::PhantomData;

use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
    CoSetProxyBlanket, CoUninitialize, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL,
    RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Variant::{
    VARIANT, VT_BOOL, VT_BSTR, VT_I2, VT_I4, VT_I8, VT_UI1, VT_UI2, VT_UI4, VT_UI8, VariantClear,
};
use windows::Win32::System::Wmi::{
    IEnumWbemClassObject, IWbemClassObject, IWbemContext, IWbemLocator, IWbemServices,
    WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_INFINITE, WbemLocator,
};
use windows::core::{BSTR, IUnknown, PCWSTR};

// `System_Rpc` is not an enabled feature for two integers; the values are
// fixed by the RPC ABI.
const RPC_C_AUTHN_WINNT: u32 = 10;
const RPC_C_AUTHZ_NONE: u32 = 0;

// `CIMTYPE` values that change how a `VARIANT` is read.
const CIM_UINT8: i32 = 17;
const CIM_UINT16: i32 = 18;
const CIM_UINT32: i32 = 19;
const CIM_SINT64: i32 = 20;
const CIM_UINT64: i32 = 21;

/// One property value, already corrected for WMI's marshalling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum WmiValue {
    Str(String),
    U64(u64),
    I64(i64),
    Bool(bool),
}

impl WmiValue {
    /// The value as text, when it is text. Numbers are not stringified:
    /// asking for a name and getting `"0"` is how placeholders leak.
    pub(super) fn text(&self) -> Option<&str> {
        match self {
            Self::Str(text) => Some(text),
            _ => None,
        }
    }

    /// The value as an unsigned number. Text is parsed as a last resort for
    /// providers that report a number as a string without saying so.
    pub(super) fn unsigned(&self) -> Option<u64> {
        match self {
            Self::U64(value) => Some(*value),
            Self::I64(value) => u64::try_from(*value).ok(),
            Self::Str(text) => text.trim().parse().ok(),
            Self::Bool(_) => None,
        }
    }

    pub(super) fn flag(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }
}

/// One instance: property name to value. A property that was null or of an
/// unsupported type is absent rather than defaulted.
pub(super) type Row = HashMap<String, WmiValue>;

/// A COM apartment that uninitialises itself only if it opened one.
struct Apartment {
    owned: bool,
}

impl Apartment {
    fn enter() -> Option<Self> {
        // SAFETY: no reserved pointer; balanced by `Drop` when owned.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if hr == RPC_E_CHANGED_MODE {
            // The thread is already an STA. Usable, but not ours to close.
            return Some(Self { owned: false });
        }
        hr.is_ok().then_some(Self { owned: true })
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        if self.owned {
            // SAFETY: balanced against the successful `CoInitializeEx`.
            unsafe { CoUninitialize() };
        }
    }
}

/// One apartment and one locator, shared by every namespace connection.
///
/// Field order is drop order: the locator is released before the apartment
/// that owns it is torn down.
pub(super) struct Wmi {
    locator: IWbemLocator,
    _apartment: Apartment,
}

impl std::fmt::Debug for Wmi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Wmi").finish_non_exhaustive()
    }
}

impl Wmi {
    pub(super) fn new() -> Option<Self> {
        let apartment = Apartment::enter()?;
        // SAFETY: `WbemLocator` is the documented CLSID; no aggregation.
        let locator: IWbemLocator =
            unsafe { CoCreateInstance(&WbemLocator, None::<&IUnknown>, CLSCTX_INPROC_SERVER) }
                .ok()?;
        Some(Self {
            locator,
            _apartment: apartment,
        })
    }

    /// Connects to `namespace` as the current user.
    pub(super) fn connect(&self, namespace: &str) -> Option<Namespace<'_>> {
        let empty = BSTR::new();
        // SAFETY: every BSTR outlives the call; empty user, password, locale
        // and authority mean the current security context.
        let services = unsafe {
            self.locator.ConnectServer(
                &BSTR::from(namespace),
                &empty,
                &empty,
                &empty,
                0,
                &empty,
                None::<&IWbemContext>,
            )
        }
        .ok()?;

        // SAFETY: `services` is a live proxy; a null principal and auth info
        // mean the process defaults.
        unsafe {
            CoSetProxyBlanket(
                &services,
                RPC_C_AUTHN_WINNT,
                RPC_C_AUTHZ_NONE,
                PCWSTR::null(),
                RPC_C_AUTHN_LEVEL_CALL,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                None,
                EOAC_NONE,
            )
        }
        .ok()?;

        Some(Namespace {
            services,
            _wmi: PhantomData,
        })
    }
}

/// A connected namespace. Borrows the [`Wmi`] so it cannot outlive the
/// apartment it was created in.
pub(super) struct Namespace<'a> {
    services: IWbemServices,
    _wmi: PhantomData<&'a Wmi>,
}

impl std::fmt::Debug for Namespace<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Namespace").finish_non_exhaustive()
    }
}

impl Namespace<'_> {
    /// `SELECT props FROM class`, one [`Row`] per instance. Any failure ends
    /// the result early; the rows already read are kept.
    pub(super) fn select(&self, class: &str, props: &[&str]) -> Vec<Row> {
        let wql = format!("SELECT {} FROM {class}", props.join(","));
        // SAFETY: both BSTRs outlive the call; no context object.
        let enumerator: Option<IEnumWbemClassObject> = unsafe {
            self.services.ExecQuery(
                &BSTR::from("WQL"),
                &BSTR::from(wql.as_str()),
                WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
                None::<&IWbemContext>,
            )
        }
        .ok();
        let Some(enumerator) = enumerator else {
            return Vec::new();
        };

        let mut rows = Vec::new();
        loop {
            let mut slot = [None];
            let mut returned = 0_u32;
            // SAFETY: `slot` has room for exactly the one object requested.
            let hr = unsafe { enumerator.Next(WBEM_INFINITE, &mut slot, &raw mut returned) };
            if hr.is_err() || returned == 0 {
                break;
            }
            let [Some(object)] = slot else { break };
            rows.push(read_row(&object, props));
        }
        rows
    }
}

fn read_row(object: &IWbemClassObject, props: &[&str]) -> Row {
    let mut row = HashMap::with_capacity(props.len());
    for &prop in props {
        let name: Vec<u16> = prop.encode_utf16().chain(Some(0)).collect();
        let mut value = VARIANT::default();
        let mut cim_type = 0_i32;
        // SAFETY: `name` is NUL-terminated and outlives the call; `value` is
        // a zeroed (VT_EMPTY) VARIANT for `Get` to fill.
        let got = unsafe {
            object.Get(
                PCWSTR(name.as_ptr()),
                0,
                &raw mut value,
                Some(&raw mut cim_type),
                None,
            )
        };
        if got.is_ok() {
            // SAFETY: `value` was initialised by a successful `Get`.
            if let Some(decoded) = unsafe { read_variant(&value, cim_type) } {
                row.insert(prop.to_owned(), decoded);
            }
        }
        // SAFETY: frees whatever `Get` stored; a no-op on VT_EMPTY. The
        // result is irrelevant: the VARIANT is discarded either way.
        let _ = unsafe { VariantClear(&raw mut value) };
    }
    row
}

/// # Safety
///
/// `value` must be a `VARIANT` initialised by `IWbemClassObject::Get`, so
/// that its `vt` tag truthfully describes the active union arm.
unsafe fn read_variant(value: &VARIANT, cim_type: i32) -> Option<WmiValue> {
    // SAFETY: the caller guarantees `vt` names the live arm, and each arm
    // below reads only the field that `vt` selects.
    unsafe {
        let inner = &value.Anonymous.Anonymous;
        let data = &inner.Anonymous;
        let raw = match inner.vt {
            VT_BSTR => return Some(from_text(cim_type, data.bstrVal.to_string())),
            VT_BOOL => return Some(WmiValue::Bool(data.boolVal.0 != 0)),
            VT_UI8 => return Some(WmiValue::U64(data.ullVal)),
            VT_UI1 => i64::from(data.bVal),
            VT_I2 => i64::from(data.iVal),
            VT_UI2 => i64::from(data.uiVal),
            VT_I4 => i64::from(data.lVal),
            VT_UI4 => i64::from(data.ulVal),
            VT_I8 => data.llVal,
            // VT_NULL, VT_EMPTY, arrays: absent, not zero.
            _ => return None,
        };
        Some(widen(cim_type, raw))
    }
}

/// Re-reads an integer as the unsigned width its CIM type declares, undoing
/// WMI's habit of marshalling `uint32` as a signed `VT_I4`.
fn widen(cim_type: i32, raw: i64) -> WmiValue {
    match cim_type {
        CIM_UINT8 => WmiValue::U64(u64::from(raw as u8)),
        CIM_UINT16 => WmiValue::U64(u64::from(raw as u16)),
        CIM_UINT32 => WmiValue::U64(u64::from(raw as u32)),
        _ => u64::try_from(raw).map_or(WmiValue::I64(raw), WmiValue::U64),
    }
}

/// Parses 64-bit integers that WMI delivers as strings; anything else stays
/// text.
fn from_text(cim_type: i32, text: String) -> WmiValue {
    match cim_type {
        CIM_UINT64 => text
            .trim()
            .parse()
            .map_or(WmiValue::Str(text), WmiValue::U64),
        CIM_SINT64 => match text.trim().parse::<i64>() {
            Ok(value) => widen(cim_type, value),
            Err(_) => WmiValue::Str(text),
        },
        _ => WmiValue::Str(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_uint32_marshalled_as_a_negative_i4_is_read_unsigned() {
        // AdapterRAM = 0xFFF00000 arrives as VT_I4 -1048576.
        assert_eq!(widen(CIM_UINT32, -1_048_576), WmiValue::U64(0xFFF0_0000),);
        assert_eq!(widen(CIM_UINT16, -1), WmiValue::U64(0xFFFF));
    }

    #[test]
    fn a_genuinely_negative_signed_value_stays_negative() {
        assert_eq!(widen(3, -5), WmiValue::I64(-5));
    }

    #[test]
    fn uint64_properties_delivered_as_strings_become_numbers() {
        assert_eq!(
            from_text(CIM_UINT64, "51539607552".to_owned()),
            WmiValue::U64(51_539_607_552),
        );
    }

    #[test]
    fn ordinary_strings_are_not_parsed_as_numbers() {
        // DeviceId "0" is a string property; it must stay text so a name is
        // never mistaken for a count.
        assert_eq!(from_text(8, "0".to_owned()), WmiValue::Str("0".to_owned()));
    }

    #[test]
    fn unsigned_reads_parse_numeric_text_but_not_booleans() {
        assert_eq!(WmiValue::Str(" 3 ".to_owned()).unsigned(), Some(3));
        assert_eq!(WmiValue::I64(-1).unsigned(), None);
        assert_eq!(WmiValue::Bool(true).unsigned(), None);
        assert_eq!(WmiValue::U64(7).text(), None);
    }
}

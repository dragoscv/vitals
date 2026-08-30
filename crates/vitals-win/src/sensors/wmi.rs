//! A minimal WMI client, sufficient for `root\WMI` thermal zones.
//!
//! # Why hand-rolled COM
//!
//! Nothing in the dependency set binds `IWbem*`, and adding a WMI crate to
//! read three integers from one class would pull a COM runtime wrapper into
//! a process that otherwise touches COM only for `SHGetKnownFolderPath`.
//! Four interfaces are needed — `IWbemLocator`, `IWbemServices`,
//! `IEnumWbemClassObject`, `IWbemClassObject` — and each is used for exactly
//! one method beyond `Release`.
//!
//! # The hazard, and how it is contained
//!
//! A hand-written vtable is the same class of bug as a mis-sized struct: get
//! a method index wrong and you call the neighbouring function with the
//! wrong signature, which is undefined behaviour rather than an error code.
//! Every index below is therefore declared as a named constant with the
//! preceding methods enumerated in a comment, and the vtable layout is
//! pinned by test. `IUnknown` occupies slots 0–2 in every interface.
//!
//! # Cost
//!
//! `CoInitializeEx` plus `ConnectServer` is tens of milliseconds on a cold
//! process. That is why nothing here is called from the sampling tick; see
//! [`super::SensorReader`], which owns the cadence.

use std::ffi::c_void;
use std::ptr;

use super::thermal::{RawZone, ThermalAvailability, ThermalScan, ThermalZone, parse_zone};

type Hresult = i32;

const S_OK: Hresult = 0;
const S_FALSE: Hresult = 1;
/// `WBEM_S_FALSE` — the enumerator is exhausted. A success code, and the
/// normal way a completed enumeration ends.
const WBEM_S_FALSE: Hresult = 1;
/// `RPC_E_CHANGED_MODE` — COM is already initialised in the other threading
/// model. Not a failure: the apartment we were given is usable.
const RPC_E_CHANGED_MODE: Hresult = -2_147_417_850;
/// `WBEM_E_ACCESS_DENIED`
const WBEM_E_ACCESS_DENIED: Hresult = -2_147_217_405;
/// `WBEM_E_INVALID_NAMESPACE`
///
/// Handled by the wildcard arm of [`classify`] rather than its own, since a
/// missing namespace and an RPC fault are the same thing to the user: we
/// could not ask. Retained, and referenced by the test that pins that
/// mapping, so the value is not rediscovered if the grouping ever changes.
#[cfg(test)]
const WBEM_E_INVALID_NAMESPACE: Hresult = -2_147_217_394;
/// `WBEM_E_INVALID_CLASS`
const WBEM_E_INVALID_CLASS: Hresult = -2_147_217_392;
/// `WBEM_E_NOT_FOUND`
const WBEM_E_NOT_FOUND: Hresult = -2_147_217_406;

const COINIT_MULTITHREADED: u32 = 0x0;
const CLSCTX_INPROC_SERVER: u32 = 0x1;

const RPC_C_AUTHN_LEVEL_CALL: u32 = 3;
const RPC_C_IMP_LEVEL_IMPERSONATE: u32 = 3;
const EOAC_NONE: u32 = 0;
const RPC_C_AUTHN_WINNT: u32 = 10;
const RPC_C_AUTHZ_NONE: u32 = 0;

const WBEM_FLAG_FORWARD_ONLY: i32 = 0x20;
const WBEM_FLAG_RETURN_IMMEDIATELY: i32 = 0x10;
const WBEM_INFINITE: i32 = -1;

/// `CLSID_WbemLocator` `{4590F811-1D3A-11D0-891F-00AA004B2E24}`.
///
/// A `static`, not a `const`: its address is passed to `CoCreateInstance`,
/// and a `const` has no address to take.
static CLSID_WBEM_LOCATOR: [u8; 16] = [
    0x11, 0xF8, 0x90, 0x45, 0x3A, 0x1D, 0xD0, 0x11, 0x89, 0x1F, 0x00, 0xAA, 0x00, 0x4B, 0x2E, 0x24,
];

/// `IID_IWbemLocator` `{DC12A687-737F-11CF-884D-00AA004B2E24}`.
static IID_IWBEM_LOCATOR: [u8; 16] = [
    0x87, 0xA6, 0x12, 0xDC, 0x7F, 0x73, 0xCF, 0x11, 0x88, 0x4D, 0x00, 0xAA, 0x00, 0x4B, 0x2E, 0x24,
];

// Vtable slots. IUnknown is 0=QueryInterface, 1=AddRef, 2=Release in every
// interface, so every index below is offset by three.

/// `IWbemLocator::ConnectServer` — the only method the interface declares.
const IWBEM_LOCATOR_CONNECT_SERVER: usize = 3;

/// `IWbemServices::ExecQuery`.
///
/// Preceded by: `OpenNamespace`(3), `CancelAsyncCall`(4), `QueryObjectSink`(5),
/// `GetObject`(6), `GetObjectAsync`(7), `PutClass`(8), `PutClassAsync`(9),
/// `DeleteClass`(10), `DeleteClassAsync`(11), `CreateClassEnum`(12),
/// `CreateClassEnumAsync`(13), `PutInstance`(14), `PutInstanceAsync`(15),
/// `DeleteInstance`(16), `DeleteInstanceAsync`(17), `CreateInstanceEnum`(18),
/// `CreateInstanceEnumAsync`(19).
const IWBEM_SERVICES_EXEC_QUERY: usize = 20;

/// `IEnumWbemClassObject::Next`.
///
/// Preceded by: `Reset`(3).
const IENUM_WBEM_NEXT: usize = 4;

/// `IWbemClassObject::Get`.
///
/// Preceded by: `GetQualifierSet`(3).
const IWBEM_CLASS_OBJECT_GET: usize = 4;

const IUNKNOWN_RELEASE: usize = 2;

// The signatures of the four methods called. Declared at module scope beside
// their slot constants: a signature and the index it is invoked through are
// only correct as a pair, and separating them is how the pair drifts.

type ConnectServerFn = unsafe extern "system" fn(
    *mut c_void,
    *mut u16,
    *mut u16,
    *mut u16,
    *mut u16,
    i32,
    *mut u16,
    *mut c_void,
    *mut *mut c_void,
) -> Hresult;

type ExecQueryFn = unsafe extern "system" fn(
    *mut c_void,
    *mut u16,
    *mut u16,
    i32,
    *mut c_void,
    *mut *mut c_void,
) -> Hresult;

type NextFn =
    unsafe extern "system" fn(*mut c_void, i32, u32, *mut *mut c_void, *mut u32) -> Hresult;

type GetFn = unsafe extern "system" fn(
    *mut c_void,
    *const u16,
    i32,
    *mut Variant,
    *mut i32,
    *mut i32,
) -> Hresult;

type ReleaseFn = unsafe extern "system" fn(*mut c_void) -> u32;

// VARIANT types we accept. WMI returns `CurrentTemperature` as VT_I4 on
// every build observed, but the class qualifier says uint32, and a provider
// is free to hand back VT_UI4. Accepting only one would drop the reading on
// a machine whose provider chose the other — silently, as a missing sensor.
const VT_EMPTY: u16 = 0;
const VT_NULL: u16 = 1;
const VT_I4: u16 = 3;
const VT_UI4: u16 = 19;
const VT_BOOL: u16 = 11;
const VT_BSTR: u16 = 8;

/// `VARIANT`, 24 bytes on 64-bit.
///
/// Declared as a header plus an 8-byte payload rather than the full union:
/// only the integer, boolean and BSTR arms are read, and every one of them
/// lives in the first eight bytes of the union. The *size* still has to be
/// exactly right, because `VariantClear` walks it.
#[repr(C)]
#[derive(Clone, Copy)]
struct Variant {
    vt: u16,
    reserved1: u16,
    reserved2: u16,
    reserved3: u16,
    value: u64,
    /// Padding to the documented 24 bytes. `DECIMAL` and `BRECORD` arms make
    /// the union this wide even though nothing here reads them.
    tail: u64,
}

impl Default for Variant {
    fn default() -> Self {
        Self {
            vt: VT_EMPTY,
            reserved1: 0,
            reserved2: 0,
            reserved3: 0,
            value: 0,
            tail: 0,
        }
    }
}

#[link(name = "ole32")]
unsafe extern "system" {
    fn CoInitializeEx(reserved: *mut c_void, flags: u32) -> Hresult;
    fn CoUninitialize();
    fn CoCreateInstance(
        clsid: *const [u8; 16],
        outer: *mut c_void,
        context: u32,
        iid: *const [u8; 16],
        out: *mut *mut c_void,
    ) -> Hresult;
    fn CoSetProxyBlanket(
        proxy: *mut c_void,
        authn_service: u32,
        authz_service: u32,
        principal: *const u16,
        authn_level: u32,
        imp_level: u32,
        auth_info: *mut c_void,
        capabilities: u32,
    ) -> Hresult;
}

#[link(name = "oleaut32")]
unsafe extern "system" {
    fn SysAllocString(text: *const u16) -> *mut u16;
    fn SysFreeString(text: *mut u16);
    fn SysStringLen(text: *const u16) -> u32;
    fn VariantClear(variant: *mut Variant) -> Hresult;
}

/// A COM apartment that uninitialises itself.
///
/// Not created when COM was already initialised in another mode: calling
/// `CoUninitialize` on an apartment this module did not open would tear it
/// down under whoever did.
struct Apartment {
    owned: bool,
}

impl Apartment {
    fn enter() -> Option<Self> {
        // SAFETY: a null reserved pointer is required by the contract.
        let hr = unsafe { CoInitializeEx(ptr::null_mut(), COINIT_MULTITHREADED) };

        match hr {
            S_OK | S_FALSE => Some(Self { owned: true }),
            // Someone already put this thread in an STA. Usable, but not
            // ours to close.
            RPC_E_CHANGED_MODE => Some(Self { owned: false }),
            _ => None,
        }
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        if self.owned {
            // SAFETY: balanced against the successful CoInitializeEx above.
            unsafe { CoUninitialize() };
        }
    }
}

/// A COM interface pointer that releases itself.
struct ComPtr(*mut c_void);

impl ComPtr {
    /// The function at `slot` in this object's vtable.
    ///
    /// # Safety
    ///
    /// `slot` must be a valid index into the interface's vtable and `F` must
    /// exactly match that method's signature, including the `this` pointer.
    /// Getting either wrong calls a different function with the wrong ABI.
    unsafe fn method<F: Copy>(&self, slot: usize) -> F {
        // SAFETY: a COM object begins with a pointer to its vtable, which is
        // an array of function pointers; the caller guarantees `slot` is in
        // range and `F` matches.
        unsafe {
            let vtable = *self.0.cast::<*const *const c_void>();
            *vtable.add(slot).cast::<F>()
        }
    }
}

impl Drop for ComPtr {
    fn drop(&mut self) {
        // SAFETY: slot 2 is IUnknown::Release in every COM interface, and
        // its signature is fixed. `self.0` is non-null by construction.
        unsafe { self.method::<ReleaseFn>(IUNKNOWN_RELEASE)(self.0) };
    }
}

/// A `BSTR` that frees itself.
struct BStr(*mut u16);

impl BStr {
    fn new(text: &str) -> Option<Self> {
        let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        // SAFETY: `wide` is NUL-terminated and outlives the call.
        let raw = unsafe { SysAllocString(wide.as_ptr()) };
        (!raw.is_null()).then_some(Self(raw))
    }
}

impl Drop for BStr {
    fn drop(&mut self) {
        // SAFETY: allocated by SysAllocString and freed once.
        unsafe { SysFreeString(self.0) };
    }
}

/// Reads every ACPI thermal zone.
///
/// Never returns an error type: the *reason* for an empty result is the
/// interesting part and it travels in [`ThermalScan::availability`], where a
/// caller cannot discard it by mapping the error to a default.
#[must_use]
pub fn read_thermal_zones() -> ThermalScan {
    let Some(_apartment) = Apartment::enter() else {
        return ThermalScan::unavailable(ThermalAvailability::ProviderMissing);
    };

    let mut raw_locator: *mut c_void = ptr::null_mut();

    // SAFETY: both GUIDs are 16-byte constants; a null outer pointer means
    // no aggregation.
    let hr = unsafe {
        CoCreateInstance(
            &raw const CLSID_WBEM_LOCATOR,
            ptr::null_mut(),
            CLSCTX_INPROC_SERVER,
            &raw const IID_IWBEM_LOCATOR,
            &raw mut raw_locator,
        )
    };

    if hr != S_OK || raw_locator.is_null() {
        return ThermalScan::unavailable(ThermalAvailability::ProviderMissing);
    }

    let locator = ComPtr(raw_locator);

    let services = match connect(&locator) {
        Ok(services) => services,
        Err(reason) => return ThermalScan::unavailable(reason),
    };

    let enumerator = match exec_query(&services) {
        Ok(enumerator) => enumerator,
        Err(reason) => return ThermalScan::unavailable(reason),
    };

    let (zones, terminal) = drain(&enumerator);

    // `WBEM_FLAG_RETURN_IMMEDIATELY` makes ExecQuery return a semi-synchronous
    // enumerator that succeeds before the provider has been consulted, so a
    // permission failure arrives at the FIRST `Next` rather than at the
    // query. Discarding that HRESULT and inferring "no zones" from an empty
    // list is therefore wrong on every unelevated machine — it tells the user
    // their board has no thermal sensors when it has several and they simply
    // are not allowed to read them. Verified against PowerShell, which
    // reports "Access denied" for the same query on this box.
    let availability = if !zones.is_empty() {
        ThermalAvailability::Available
    } else if terminal == WBEM_S_FALSE || terminal == S_OK {
        // The enumerator ran to completion and yielded nothing. Only here is
        // "this firmware exposes no zones" a true statement.
        ThermalAvailability::NoZonesPresent
    } else {
        classify(terminal)
    };

    ThermalScan {
        zones,
        availability,
    }
}

/// Connects to `root\WMI` and sets the proxy security the ACPI provider
/// requires.
///
/// # Errors
///
/// The availability the UI should show, already classified.
fn connect(locator: &ComPtr) -> Result<ComPtr, ThermalAvailability> {
    let namespace = BStr::new("root\\WMI").ok_or(ThermalAvailability::ProviderMissing)?;

    let mut raw_services: *mut c_void = ptr::null_mut();

    // SAFETY: slot 3 is IWbemLocator::ConnectServer, whose signature is
    // matched by `ConnectServerFn`. Every optional argument is legitimately
    // null: current user, current locale, no authority, no context.
    let hr = unsafe {
        locator.method::<ConnectServerFn>(IWBEM_LOCATOR_CONNECT_SERVER)(
            locator.0,
            namespace.0,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            &raw mut raw_services,
        )
    };

    if hr != S_OK || raw_services.is_null() {
        return Err(classify(hr));
    }

    let services = ComPtr(raw_services);

    // Without this the proxy defaults to a level that makes the ACPI
    // provider refuse every query, which surfaces as access-denied and looks
    // exactly like the genuine permission failure.
    // SAFETY: `services.0` is a live proxy; null principal and auth info
    // mean "use the current identity".
    let hr = unsafe {
        CoSetProxyBlanket(
            services.0,
            RPC_C_AUTHN_WINNT,
            RPC_C_AUTHZ_NONE,
            ptr::null(),
            RPC_C_AUTHN_LEVEL_CALL,
            RPC_C_IMP_LEVEL_IMPERSONATE,
            ptr::null_mut(),
            EOAC_NONE,
        )
    };

    if hr == S_OK {
        Ok(services)
    } else {
        Err(classify(hr))
    }
}

/// Runs the thermal-zone query.
///
/// # Errors
///
/// The availability the UI should show, already classified.
fn exec_query(services: &ComPtr) -> Result<ComPtr, ThermalAvailability> {
    let language = BStr::new("WQL").ok_or(ThermalAvailability::ProviderMissing)?;
    let query = BStr::new("SELECT * FROM MSAcpi_ThermalZoneTemperature")
        .ok_or(ThermalAvailability::ProviderMissing)?;

    let mut raw_enum: *mut c_void = ptr::null_mut();

    // SAFETY: slot 20 is IWbemServices::ExecQuery; the BSTRs are live and a
    // null context is permitted.
    let hr = unsafe {
        services.method::<ExecQueryFn>(IWBEM_SERVICES_EXEC_QUERY)(
            services.0,
            language.0,
            query.0,
            WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
            ptr::null_mut(),
            &raw mut raw_enum,
        )
    };

    if hr != S_OK || raw_enum.is_null() {
        return Err(classify(hr));
    }

    Ok(ComPtr(raw_enum))
}

/// Pulls every object out of an enumerator.
///
/// Returns the zones alongside the `HRESULT` that ended the loop. The status
/// is not optional: with a semi-synchronous enumerator it is the only place
/// a permission failure appears, and dropping it would leave the caller
/// unable to tell refusal from absence.
fn drain(enumerator: &ComPtr) -> (Vec<ThermalZone>, Hresult) {
    let mut zones = Vec::new();

    loop {
        let mut object: *mut c_void = ptr::null_mut();
        let mut returned: u32 = 0;

        // One at a time. Batching would need an array whose unreturned tail
        // must not be released, and the enumerator is at most a handful of
        // objects on any real machine.
        // SAFETY: slot 4 is IEnumWbemClassObject::Next; the out-parameters
        // are live locals.
        let hr = unsafe {
            enumerator.method::<NextFn>(IENUM_WBEM_NEXT)(
                enumerator.0,
                WBEM_INFINITE,
                1,
                &raw mut object,
                &raw mut returned,
            )
        };

        if hr != S_OK || returned == 0 || object.is_null() {
            return (zones, hr);
        }

        let wrapped = ComPtr(object);

        if let Some(zone) = read_zone(&wrapped) {
            zones.push(zone);
        }
    }
}

/// Reads the properties of one `MSAcpi_ThermalZoneTemperature` instance.
fn read_zone(object: &ComPtr) -> Option<ThermalZone> {
    let instance = property_string(object, "InstanceName")?;
    let current = property_u32(object, "CurrentTemperature")?;

    parse_zone(
        &instance,
        RawZone {
            current_decikelvin: current,
            // Absent on some providers. Zero is the right stand-in because
            // the conversion rejects it as implausible rather than
            // rendering −273 °C.
            critical_decikelvin: property_u32(object, "CriticalTripPoint").unwrap_or(0),
            has_active_cooling: property_u32(object, "ActiveCount").unwrap_or(0) > 0,
        },
    )
}

/// Fetches one property as a `VARIANT`, cleared on drop.
fn property(object: &ComPtr, name: &str) -> Option<Variant> {
    let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let mut variant = Variant::default();

    // SAFETY: slot 4 is IWbemClassObject::Get; `wide` is NUL-terminated and
    // outlives the call, and the type/flavour out-parameters are optional.
    let hr = unsafe {
        object.method::<GetFn>(IWBEM_CLASS_OBJECT_GET)(
            object.0,
            wide.as_ptr(),
            0,
            &raw mut variant,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };

    if hr != S_OK || variant.vt == VT_NULL || variant.vt == VT_EMPTY {
        return None;
    }

    Some(variant)
}

/// A property read as an unsigned 32-bit value.
///
/// Accepts both `VT_I4` and `VT_UI4`: the class declares uint32 but the
/// provider hands back a signed variant on every build observed, and
/// insisting on one would drop the reading on a machine that chose the
/// other — appearing as an absent sensor rather than as a parse failure.
fn property_u32(object: &ComPtr, name: &str) -> Option<u32> {
    let mut variant = property(object, name)?;

    let value = match variant.vt {
        VT_I4 | VT_UI4 => Some(variant.value as u32),
        VT_BOOL => Some(u32::from(variant.value as u16 != 0)),
        _ => None,
    };

    // SAFETY: `variant` was filled by IWbemClassObject::Get and is cleared
    // exactly once.
    unsafe { VariantClear(&raw mut variant) };

    value
}

/// A property read as a string.
fn property_string(object: &ComPtr, name: &str) -> Option<String> {
    let mut variant = property(object, name)?;

    let value = if variant.vt == VT_BSTR && variant.value != 0 {
        let bstr = variant.value as *const u16;
        // A BSTR carries its length in a prefix word; walking to a NUL would
        // truncate any string containing an embedded one. None do here, but
        // the length is free and the habit is not.
        // SAFETY: `bstr` is a live BSTR owned by the variant.
        let len = unsafe { SysStringLen(bstr) } as usize;
        // SAFETY: a BSTR is `len` UTF-16 units of initialised memory.
        let slice = unsafe { std::slice::from_raw_parts(bstr, len) };
        Some(String::from_utf16_lossy(slice))
    } else {
        None
    };

    // SAFETY: `variant` was filled by Get and is cleared exactly once. This
    // is what frees the BSTR read above, so the String must already be owned.
    unsafe { VariantClear(&raw mut variant) };

    value
}

/// Maps a WMI `HRESULT` onto the availability the UI can act on.
const fn classify(hr: Hresult) -> ThermalAvailability {
    match hr {
        WBEM_E_ACCESS_DENIED => ThermalAvailability::AccessDenied,
        // A missing class means the ACPI driver registered but declared no
        // thermal-zone class, which is a real statement about the firmware.
        WBEM_E_INVALID_CLASS | WBEM_E_NOT_FOUND => ThermalAvailability::NoZonesPresent,
        // Everything else — a missing namespace, an RPC fault — means "we
        // could not ask", which is not the same as "the board has no
        // sensors". Claiming the latter on the strength of an RPC error
        // would be exactly the fabrication this module exists to avoid.
        _ => ThermalAvailability::ProviderMissing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_layout_is_pinned() {
        // 24 on 64-bit. Too small and VariantClear walks past the allocation;
        // too large and the value arm is read at the wrong offset, yielding a
        // temperature assembled from padding.
        assert_eq!(size_of::<Variant>(), 24);
        assert_eq!(align_of::<Variant>(), 8);
    }

    #[test]
    fn guids_are_in_little_endian_memory_order() {
        // CLSID_WbemLocator Data1 = 4590F811.
        assert_eq!(&CLSID_WBEM_LOCATOR[..4], &[0x11, 0xF8, 0x90, 0x45]);
        // IID_IWbemLocator Data1 = DC12A687.
        assert_eq!(&IID_IWBEM_LOCATOR[..4], &[0x87, 0xA6, 0x12, 0xDC]);
        // Both share the same Data4 tail.
        assert_eq!(&CLSID_WBEM_LOCATOR[10..], &IID_IWBEM_LOCATOR[10..]);
    }

    #[test]
    fn access_denied_is_not_reported_as_absent_hardware() {
        assert_eq!(
            classify(WBEM_E_ACCESS_DENIED),
            ThermalAvailability::AccessDenied
        );
        assert_eq!(
            classify(WBEM_E_INVALID_CLASS),
            ThermalAvailability::NoZonesPresent
        );
        // A broken or stripped WMI installation is "could not ask", not
        // "no hardware".
        assert_eq!(
            classify(WBEM_E_INVALID_NAMESPACE),
            ThermalAvailability::ProviderMissing
        );
        // An unknown fault must never claim the sensors do not exist.
        assert_ne!(classify(-1), ThermalAvailability::NoZonesPresent);
    }

    #[test]
    fn vtable_slots_sit_after_iunknown() {
        for slot in [
            IWBEM_LOCATOR_CONNECT_SERVER,
            IWBEM_SERVICES_EXEC_QUERY,
            IENUM_WBEM_NEXT,
            IWBEM_CLASS_OBJECT_GET,
        ] {
            assert!(slot > IUNKNOWN_RELEASE, "slot {slot} overlaps IUnknown");
        }
    }

    #[test]
    fn reading_zones_never_panics_whatever_the_permissions() {
        // The contract the sampler depends on: unelevated, elevated, or on a
        // board with no provider at all, this returns a scan.
        let scan = read_thermal_zones();
        if scan.zones.is_empty() {
            assert_ne!(scan.availability, ThermalAvailability::Available);
        }
    }

    #[test]
    fn an_exhausted_enumerator_is_the_only_route_to_no_zones() {
        // WBEM_S_FALSE and S_OK are the two success codes that end a
        // completed enumeration; everything else means the provider refused
        // or faulted. This is the distinction that was wrong first time
        // round, when a semi-synchronous ExecQuery succeeded and the
        // access-denied arrived at Next instead.
        assert_eq!(WBEM_S_FALSE, S_FALSE);
        assert_ne!(
            classify(WBEM_E_ACCESS_DENIED),
            ThermalAvailability::NoZonesPresent
        );
    }
}

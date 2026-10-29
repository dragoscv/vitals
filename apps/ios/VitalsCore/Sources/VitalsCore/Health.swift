// The owner's personal health, read from vmui (docs/api/vmui-health.md).
//
// Not the LAN client: vmui is a server on the internet (or localhost while
// developing), so there is no LAN gate and no private-address check. The
// other rules are the same as VitalsClient's:
// - The token is only ever sent in the Authorization header.
// - Redirects are never followed, so the bearer token is never replayed at
//   a host the answer names.
// - Ephemeral session: no cookies, no cache, nothing of the body on disk.
// Decoding is lenient: a missing, null or mistyped reading becomes nil (an
// em dash on screen), never 0, and never fails the whole answer.

import Foundation

// MARK: Model

extension KeyedDecodingContainer {
    /// nil for a missing key, a null, or a value of the wrong type.
    func lenient<T: Decodable>(_ type: T.Type, _ key: Key) -> T? {
        // try? flattens the optional, so a throw and a missing key both give nil.
        try? decodeIfPresent(type, forKey: key)
    }
}

public struct HrPoint: Codable, Sendable, Hashable {
    /// Epoch milliseconds.
    public let t: Double
    /// Beats per minute.
    public let v: Double

    public init(t: Double, v: Double) {
        self.t = t
        self.v = v
    }
}

/// A heart-rate point as sent: either field may be absent, and such a point
/// is dropped rather than drawn at zero.
struct LenientPoint: Decodable {
    let t: Double?
    let v: Double?

    enum CodingKeys: String, CodingKey { case t, v }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        t = c.lenient(Double.self, .t)
        v = c.lenient(Double.self, .v)
    }
}

public struct BloodPressure: Codable, Sendable, Hashable {
    public let sys: Double?
    public let dia: Double?

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        sys = c.lenient(Double.self, .sys)
        dia = c.lenient(Double.self, .dia)
    }
}

/// Values vmui estimates from height and weight; always labelled "estimate".
public struct BodyEstimate: Codable, Sendable, Hashable {
    public let bmi: Double?
    public let fatPct: Double?
    public let fatKg: Double?
    public let waterKg: Double?
    public let waterPct: Double?
    public let leanKg: Double?
    public let muscleKg: Double?
    public let bmrKcal: Double?
    public let boneKg: Double?
    public let ageYears: Double?
    public let bmiBand: String?

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        bmi = c.lenient(Double.self, .bmi)
        fatPct = c.lenient(Double.self, .fatPct)
        fatKg = c.lenient(Double.self, .fatKg)
        waterKg = c.lenient(Double.self, .waterKg)
        waterPct = c.lenient(Double.self, .waterPct)
        leanKg = c.lenient(Double.self, .leanKg)
        muscleKg = c.lenient(Double.self, .muscleKg)
        bmrKcal = c.lenient(Double.self, .bmrKcal)
        boneKg = c.lenient(Double.self, .boneKg)
        ageYears = c.lenient(Double.self, .ageYears)
        bmiBand = c.lenient(String.self, .bmiBand)
    }
}

/// Readings from the owner's phone, as Home Assistant last saw them.
public struct PhoneHealth: Codable, Sendable {
    public let device: String?
    public let steps: Double?
    public let stepsAt: Double?
    public let distanceKm: Double?
    public let floors: Double?
    public let activeKcal: Double?
    public let totalKcal: Double?
    public let heartRate: Double?
    public let heartRateAt: Double?
    public let restingHr: Double?
    public let hrv: Double?
    public let spo2: Double?
    public let respiratoryRate: Double?
    public let sleepMin: Double?
    public let sleepAt: Double?
    public let weightKg: Double?
    public let weightAt: Double?
    public let bodyFat: Double?
    public let body: BodyEstimate?
    public let bmr: Double?
    public let vo2max: Double?
    public let bloodPressure: BloodPressure?
    public let hrHistory: [HrPoint]

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        device = c.lenient(String.self, .device)
        steps = c.lenient(Double.self, .steps)
        stepsAt = c.lenient(Double.self, .stepsAt)
        distanceKm = c.lenient(Double.self, .distanceKm)
        floors = c.lenient(Double.self, .floors)
        activeKcal = c.lenient(Double.self, .activeKcal)
        totalKcal = c.lenient(Double.self, .totalKcal)
        heartRate = c.lenient(Double.self, .heartRate)
        heartRateAt = c.lenient(Double.self, .heartRateAt)
        restingHr = c.lenient(Double.self, .restingHr)
        hrv = c.lenient(Double.self, .hrv)
        spo2 = c.lenient(Double.self, .spo2)
        respiratoryRate = c.lenient(Double.self, .respiratoryRate)
        sleepMin = c.lenient(Double.self, .sleepMin)
        sleepAt = c.lenient(Double.self, .sleepAt)
        weightKg = c.lenient(Double.self, .weightKg)
        weightAt = c.lenient(Double.self, .weightAt)
        bodyFat = c.lenient(Double.self, .bodyFat)
        body = c.lenient(BodyEstimate.self, .body)
        bmr = c.lenient(Double.self, .bmr)
        vo2max = c.lenient(Double.self, .vo2max)
        bloodPressure = c.lenient(BloodPressure.self, .bloodPressure)
        hrHistory = (c.lenient([LenientPoint].self, .hrHistory) ?? []).compactMap { p in
            guard let t = p.t, let v = p.v, t.isFinite, v.isFinite else { return nil }
            return HrPoint(t: t, v: v)
        }
    }
}

public struct NutritionTargets: Codable, Sendable {
    public let weightKg: Double?
    public let weightSource: String?
    public let ageYears: Double?
    public let bmr: Double?
    public let tdee: Double?
    public let calories: Double?
    public let protein: Double?
    public let carbs: Double?
    public let fats: Double?
    public let fiber: Double?

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        weightKg = c.lenient(Double.self, .weightKg)
        weightSource = c.lenient(String.self, .weightSource)
        ageYears = c.lenient(Double.self, .ageYears)
        bmr = c.lenient(Double.self, .bmr)
        tdee = c.lenient(Double.self, .tdee)
        calories = c.lenient(Double.self, .calories)
        protein = c.lenient(Double.self, .protein)
        carbs = c.lenient(Double.self, .carbs)
        fats = c.lenient(Double.self, .fats)
        fiber = c.lenient(Double.self, .fiber)
    }
}

public struct NutritionDay: Codable, Sendable {
    public let day: String?
    public let calories: Double?
    public let protein: Double?
    public let carbs: Double?
    public let fats: Double?
    public let fiber: Double?
    public let meals: Double?

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        day = c.lenient(String.self, .day)
        calories = c.lenient(Double.self, .calories)
        protein = c.lenient(Double.self, .protein)
        carbs = c.lenient(Double.self, .carbs)
        fats = c.lenient(Double.self, .fats)
        fiber = c.lenient(Double.self, .fiber)
        meals = c.lenient(Double.self, .meals)
    }
}

/// What is left of today's targets; may be negative.
public struct NutritionRemaining: Codable, Sendable {
    public let calories: Double?
    public let protein: Double?
    public let carbs: Double?
    public let fats: Double?

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        calories = c.lenient(Double.self, .calories)
        protein = c.lenient(Double.self, .protein)
        carbs = c.lenient(Double.self, .carbs)
        fats = c.lenient(Double.self, .fats)
    }
}

public struct WaterIntake: Codable, Sendable {
    public let day: String?
    public let ml: Double?
    public let glasses: Double?
    public let targetMl: Double?
    public let remainingMl: Double?
    public let lastAt: Double?
    public let underPace: Bool?

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        day = c.lenient(String.self, .day)
        ml = c.lenient(Double.self, .ml)
        glasses = c.lenient(Double.self, .glasses)
        targetMl = c.lenient(Double.self, .targetMl)
        remainingMl = c.lenient(Double.self, .remainingMl)
        lastAt = c.lenient(Double.self, .lastAt)
        underPace = c.lenient(Bool.self, .underPace)
    }
}

public struct Nutrition: Codable, Sendable {
    public let day: String?
    public let targets: NutritionTargets?
    public let today: NutritionDay?
    public let remaining: NutritionRemaining?
    public let balance: Double?
    public let activeKcal: Double?
    public let water: WaterIntake?

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        day = c.lenient(String.self, .day)
        targets = c.lenient(NutritionTargets.self, .targets)
        today = c.lenient(NutritionDay.self, .today)
        remaining = c.lenient(NutritionRemaining.self, .remaining)
        balance = c.lenient(Double.self, .balance)
        activeKcal = c.lenient(Double.self, .activeKcal)
        water = c.lenient(WaterIntake.self, .water)
    }
}

/// `GET /api/v1/me/health`. Kept in memory only: never written to disk.
public struct MeHealth: Codable, Sendable {
    /// Epoch milliseconds, when vmui built the answer.
    public let at: Double?
    /// nil: Home Assistant is down or the phone publishes no sensors.
    public let health: PhoneHealth?
    public let nutrition: Nutrition?

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        at = c.lenient(Double.self, .at)
        health = c.lenient(PhoneHealth.self, .health)
        nutrition = c.lenient(Nutrition.self, .nutrition)
    }
}

// MARK: Client

public enum HealthFailure: Error, Equatable, Sendable {
    /// 401: the token is unknown or revoked. Pair again; keep the base URL.
    case signedOut
    /// 403: the device belongs to someone who is not the owner.
    case notOwner
    /// Network error or any other status.
    case unreachable
    /// A 200 whose body is not the contract.
    case invalid
    /// The base URL is not https (or http on localhost).
    case badAddress
    /// Sign-in refused the email and password.
    case wrongCredentials
    /// 429.
    case slowDown
}

public enum HealthPairStatus: Sendable, Equatable {
    case pending, approved, rejected, expired
}

/// The answer to starting a code pairing. The token only works once the
/// owner approves `code` in vmui.
public struct HealthPairTicket: Sendable, Equatable {
    public let id: String
    public let token: String
    public let code: String
}

public struct HealthClient: Sendable {
    public static let platform = "vitals-ios"

    let session: URLSession

    static let sharedSession: URLSession = {
        let c = URLSessionConfiguration.ephemeral
        c.timeoutIntervalForRequest = 10
        c.timeoutIntervalForResource = 15
        c.waitsForConnectivity = false
        c.httpShouldSetCookies = false
        c.httpCookieAcceptPolicy = .never
        c.urlCache = nil
        c.requestCachePolicy = .reloadIgnoringLocalCacheData
        return URLSession(configuration: c, delegate: NoRedirects(), delegateQueue: nil)
    }()

    public init(session: URLSession? = nil) {
        self.session = session ?? Self.sharedSession
    }

    /// `https://host[:port][/path]` without a trailing slash, or nil. Plain
    /// http is accepted only for localhost and 127.0.0.1 (development).
    public static func normalise(_ raw: String) -> String? {
        var trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        // A bare host means https; it never silently becomes http.
        if !trimmed.isEmpty, !trimmed.contains("://") { trimmed = "https://" + trimmed }
        guard var comps = URLComponents(string: trimmed),
              let scheme = comps.scheme?.lowercased(),
              let host = comps.host?.lowercased(), !host.isEmpty,
              comps.user == nil, comps.password == nil,
              comps.query == nil, comps.fragment == nil
        else { return nil }
        switch scheme {
        case "https": break
        case "http" where host == "localhost" || host == "127.0.0.1": break
        default: return nil
        }
        comps.scheme = scheme
        comps.host = host
        while comps.path.hasSuffix("/") { comps.path.removeLast() }
        return comps.string
    }

    struct PairBody: Encodable {
        let name: String
        let platform: String
        var email: String?
        var password: String?
    }

    struct PairReply: Decodable {
        let id: String?
        let token: String?
        let code: String?
        let status: String?
    }

    /// Step 1 of code pairing: show `code`, then poll with `id`.
    public func pairStart(base: String, name: String) async -> Result<HealthPairTicket, HealthFailure> {
        let body = PairBody(name: String(name.prefix(64)), platform: Self.platform)
        switch await post(base: base, body: body) {
        case let .failure(f): return .failure(f)
        case let .success((status, data)):
            switch status {
            case 200..<300:
                guard let r = try? JSONDecoder().decode(PairReply.self, from: data),
                      let id = r.id, let token = r.token, let code = r.code
                else { return .failure(.invalid) }
                return .success(HealthPairTicket(id: id, token: token, code: code))
            case 429: return .failure(.slowDown)
            default: return .failure(.unreachable)
            }
        }
    }

    /// Step 2: asked every 3 s until approved, rejected or expired.
    public func pairPoll(base: String, id: String) async -> Result<HealthPairStatus, HealthFailure> {
        guard let root = Self.normalise(base), var comps = URLComponents(string: root + "/api/devices/pair")
        else { return .failure(.badAddress) }
        comps.queryItems = [URLQueryItem(name: "id", value: id)]
        guard let url = comps.url else { return .failure(.badAddress) }
        var req = URLRequest(url: url)
        req.setValue("application/json", forHTTPHeaderField: "Accept")
        switch await send(req) {
        case let .failure(f): return .failure(f)
        case let .success((status, data)):
            switch status {
            case 200:
                guard let r = try? JSONDecoder().decode(PairReply.self, from: data) else { return .failure(.invalid) }
                switch r.status ?? "" {
                case "approved": return .success(.approved)
                case "rejected": return .success(.rejected)
                case "expired": return .success(.expired)
                default: return .success(.pending)
                }
            case 404: return .success(.expired)
            case 429: return .failure(.slowDown)
            default: return .failure(.unreachable)
            }
        }
    }

    /// Instant pairing with the owner's vmui account. The password is sent
    /// once, in the body, and the caller must not keep it.
    public func signIn(base: String, email: String, password: String) async -> Result<String, HealthFailure> {
        await signIn(base: base, name: "iPhone", email: email, password: password)
    }

    public func signIn(base: String, name: String, email: String, password: String) async -> Result<String, HealthFailure> {
        let body = PairBody(name: String(name.prefix(64)), platform: Self.platform, email: email, password: password)
        switch await post(base: base, body: body) {
        case let .failure(f): return .failure(f)
        case let .success((status, data)):
            switch status {
            case 200..<300:
                guard let r = try? JSONDecoder().decode(PairReply.self, from: data),
                      let token = r.token, r.status == "approved"
                else { return .failure(.invalid) }
                return .success(token)
            case 401: return .failure(.wrongCredentials)
            case 429: return .failure(.slowDown)
            default: return .failure(.unreachable)
            }
        }
    }

    /// `GET /api/v1/me/health`, the token in the Authorization header only.
    public func fetch(base: String, token: String) async -> Result<MeHealth, HealthFailure> {
        guard let root = Self.normalise(base), let url = URL(string: root + "/api/v1/me/health")
        else { return .failure(.badAddress) }
        var req = URLRequest(url: url)
        req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        req.setValue("application/json", forHTTPHeaderField: "Accept")
        switch await send(req) {
        case let .failure(f): return .failure(f)
        case let .success((status, data)):
            switch status {
            case 200:
                guard let h = try? JSONDecoder().decode(MeHealth.self, from: data) else { return .failure(.invalid) }
                return .success(h)
            case 401: return .failure(.signedOut)
            case 403: return .failure(.notOwner)
            default: return .failure(.unreachable)
            }
        }
    }

    private func post(base: String, body: PairBody) async -> Result<(Int, Data), HealthFailure> {
        guard let root = Self.normalise(base), let url = URL(string: root + "/api/devices/pair")
        else { return .failure(.badAddress) }
        var req = URLRequest(url: url)
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.setValue("application/json", forHTTPHeaderField: "Accept")
        guard let data = try? JSONEncoder().encode(body) else { return .failure(.invalid) }
        req.httpBody = data
        return await send(req)
    }

    private func send(_ req: URLRequest) async -> Result<(Int, Data), HealthFailure> {
        do {
            let (data, response) = try await session.data(for: req)
            return .success(((response as? HTTPURLResponse)?.statusCode ?? 0, data))
        } catch {
            return .failure(.unreachable)
        }
    }
}

// MARK: Formatting

/// Health numbers: nil (unmeasured) is an em dash, never "0".
public enum HealthFormat {
    static func number(_ v: Double, digits: Int, locale: Locale) -> String {
        v.formatted(.number.precision(.fractionLength(digits)).locale(locale))
    }

    /// A reading with its unit: "6.1 km", "72 bpm".
    public static func value(_ v: Double?, _ unit: String, digits: Int = 0, locale: Locale = .current) -> String {
        guard let v, v.isFinite else { return Format.dash }
        let n = number(v, digits: digits, locale: locale)
        return unit.isEmpty ? n : "\(n) \(unit)"
    }

    /// "98 / 160 g": either side may be a dash on its own.
    public static func of(_ v: Double?, _ target: Double?, _ unit: String, locale: Locale = .current) -> String {
        if v == nil, target == nil { return Format.dash }
        let a = value(v, "", locale: locale)
        let b = value(target, "", locale: locale)
        return unit.isEmpty ? "\(a) / \(b)" : "\(a) / \(b) \(unit)"
    }

    /// Minutes asleep as "6h 52m".
    public static func sleep(_ minutes: Double?) -> String {
        guard let minutes, minutes.isFinite, minutes >= 0 else { return Format.dash }
        return Format.duration(Int64((minutes * 60).rounded()))
    }

    /// "121/79 mmHg"; a pressure with either side missing is unmeasured.
    public static func pressure(_ bp: BloodPressure?) -> String {
        guard let s = bp?.sys, let d = bp?.dia, s.isFinite, d.isFinite else { return Format.dash }
        return "\(Int(s.rounded()))/\(Int(d.rounded())) mmHg"
    }

    /// A fraction for a progress bar; nil when either side is unmeasured or
    /// the target is not positive.
    public static func fraction(_ v: Double?, _ target: Double?) -> Double? {
        guard let v, let target, v.isFinite, target.isFinite, target > 0 else { return nil }
        return v / target
    }
}

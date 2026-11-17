// One smoke test per screen the iPhone app has without a paired PC: the
// machines list, the Sample PC's Now / Sensors / History, its Programs list,
// and Settings. Health needs a paired vmui (and a password), so it is covered
// by the VitalsCore unit tests and by the review path, not here.
//
// Every screen is reached through the app's own launch arguments — the ones
// apps/ios/scripts/screenshots.sh already uses — rather than by tapping. That
// matters on a Mac with no GPU: iOS 26 then draws the glass tab bar with only
// its selected item, so a tap on "Settings" is unreliable there, while a
// launch argument is not. The language is pinned to en-GB so the assertions
// read the catalog's English rather than whatever the host is set to.
//
// Run on a Mac:  bash apps/ios/scripts/build.sh uitest

import XCTest

final class VitalsSmokeTests: XCTestCase {
    /// Force English: `L()` reads the catalog, and the assertions below are its
    /// English strings.
    private static let english = ["-AppleLanguages", "(en-GB)", "-AppleLocale", "en_GB"]

    override func setUp() {
        super.setUp()
        continueAfterFailure = false
    }

    private func launch(_ extra: String...) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = Self.english + extra
        app.launch()
        return app
    }

    /// Machines: with nothing paired the empty state offers the sample, and the
    /// toolbar still offers Add PC.
    func testTheMachinesTabOffersTheSampleAndAddPc() {
        let app = launch()
        XCTAssertTrue(app.navigationBars["Your PCs"].waitForExistence(timeout: 30))
        XCTAssertTrue(app.buttons["Try with sample data"].firstMatch.exists)
        XCTAssertTrue(app.buttons["Add PC"].firstMatch.exists)
    }

    /// Settings, reached by launch argument because the tab bar is unreliable
    /// without a GPU.
    func testTheSettingsTabRenders() {
        let app = launch("-vitalsRootTab", "1")
        XCTAssertTrue(app.navigationBars["Settings"].waitForExistence(timeout: 30))
        XCTAssertTrue(app.staticTexts["Allow local network access"].firstMatch.exists)
    }

    /// The Sample PC detail opens on Now, with the three-way picker and the
    /// reading rings.
    func testTheSamplePcOpensOnNow() {
        let app = launch("-vitalsRoute", "sample", "-vitalsTab", "0")
        XCTAssertTrue(app.navigationBars["Sample PC"].waitForExistence(timeout: 30))
        let picker = app.segmentedControls.firstMatch
        XCTAssertTrue(picker.waitForExistence(timeout: 10))
        XCTAssertEqual(picker.buttons.count, 3, "the Now/Sensors/History picker is not three segments")
        XCTAssertTrue(picker.buttons["Now"].isSelected)
        XCTAssertTrue(app.staticTexts["CPU"].firstMatch.exists)
        XCTAssertTrue(app.staticTexts["Sample data, not a real PC. Pair your own PC to see its real readings."].firstMatch.exists)
    }

    /// Sensors lists the sample lines, grouped.
    func testTheSensorsTabListsSampleSensors() {
        let app = launch("-vitalsRoute", "sample", "-vitalsTab", "1")
        XCTAssertTrue(app.navigationBars["Sample PC"].waitForExistence(timeout: 30))
        XCTAssertTrue(app.staticTexts["CPU package"].waitForExistence(timeout: 15))
        XCTAssertTrue(app.staticTexts["Temperatures"].firstMatch.exists)
    }

    /// History shows the range picker and at least one chart.
    func testTheHistoryTabShowsTheRangePicker() {
        let app = launch("-vitalsRoute", "sample", "-vitalsTab", "2")
        XCTAssertTrue(app.navigationBars["Sample PC"].waitForExistence(timeout: 30))
        XCTAssertTrue(app.buttons["1 hour"].waitForExistence(timeout: 15))
        XCTAssertTrue(app.buttons["24 hours"].exists)
        XCTAssertTrue(app.buttons["7 days"].exists)
    }

    /// Programs: the list of the Sample PC opens with its sort picker.
    func testTheProgramsListOpens() {
        let app = launch("-vitalsRoute", "processes")
        XCTAssertTrue(app.navigationBars["Programs on Sample PC"].waitForExistence(timeout: 30))
        let sort = app.segmentedControls.firstMatch
        XCTAssertTrue(sort.waitForExistence(timeout: 10), "the sort picker is missing")
        XCTAssertTrue(sort.buttons["CPU"].exists)
    }
}

import XCTest
@testable import GardnMenu

final class HostTerminalTests: XCTestCase {
    func testBundledCLIIsRecognizedAsGardnClientProcess() {
        XCTAssertTrue(HostTerminal.isGardnClientProcess("gardn-cli"))
        XCTAssertTrue(HostTerminal.isGardnClientProcess("gardn"))
        XCTAssertTrue(HostTerminal.isGardnClientProcess("gardn-dev"))
        XCTAssertFalse(HostTerminal.isGardnClientProcess("Gardn"))
        XCTAssertFalse(HostTerminal.isGardnClientProcess("gardn-helper"))
    }
}

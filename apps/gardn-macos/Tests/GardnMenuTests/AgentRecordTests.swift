import XCTest
@testable import GardnMenu

final class AgentRecordTests: XCTestCase {
    func testReviewedBlockedAgentUsesBlockedSectionWithoutAttention() {
        let agent = AgentRecord(
            terminalId: "terminal-1",
            title: "Reviewed blocker",
            groupName: nil,
            groupAccent: nil,
            status: .blocked,
            statusLabel: "Blocked",
            age: nil,
            followUp: false,
            inTriage: false,
            focused: false
        )

        XCTAssertEqual(agent.section, .blocked)
        XCTAssertFalse(agent.needsAttention)
    }

    func testPendingBlockedAgentRemainsInTriageWithAttention() {
        let agent = AgentRecord(
            terminalId: "terminal-1",
            title: "Pending blocker",
            groupName: nil,
            groupAccent: nil,
            status: .blocked,
            statusLabel: "Blocked",
            age: nil,
            followUp: false,
            inTriage: true,
            focused: false
        )

        XCTAssertEqual(agent.section, .triage)
        XCTAssertTrue(agent.needsAttention)
    }
}

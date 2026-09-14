import XCTest
@testable import GardnMenu

@MainActor
final class CoordinatorCatalogTests: XCTestCase {
    func testDefaultLocalCoordinatorUsesInstanceNameInsteadOfHostname() {
        let coordinator = ExtraCoordinator(
            id: "local:default",
            kind: .local,
            name: "eva-00",
            running: true,
            socketPath: "/tmp/default.sock",
            target: nil,
            session: "default"
        )

        XCTAssertEqual(coordinator.title, "Default")
        XCTAssertEqual(coordinator.subtitle, "Local coordinator · running")
    }

    func testRefreshBeforeDisplayRemovesSelectedDeletedLocalSession() async {
        let defaultCoordinator = ExtraCoordinator(
            id: "local:default",
            kind: .local,
            name: "eva-00",
            running: true,
            socketPath: "/tmp/default.sock",
            target: nil,
            session: "default"
        )
        let qaCoordinator = ExtraCoordinator(
            id: "local:release-qa",
            kind: .local,
            name: "release-qa",
            running: false,
            socketPath: "/tmp/release-qa.sock",
            target: nil,
            session: "release-qa"
        )
        var localCoordinators = [defaultCoordinator, qaCoordinator]
        var persistedSelections: [String] = []
        let catalog = CoordinatorCatalog(
            localLoader: { localCoordinators },
            remoteRecords: [],
            selectedId: qaCoordinator.id,
            persistSelection: { persistedSelections.append($0) }
        )
        await catalog.refreshLocals()
        let store = AgentStore(socketPath: defaultCoordinator.socketPath!, catalog: catalog)
        XCTAssertEqual(catalog.coordinators.map(\.title), ["Default", "release-qa"])
        XCTAssertEqual(catalog.selectedId, qaCoordinator.id)

        localCoordinators = [defaultCoordinator]
        let refresh = store.refreshCoordinatorCatalog()
        XCTAssertTrue(catalog.isRefreshingLocals)
        await refresh.value

        XCTAssertEqual(catalog.coordinators.map(\.title), ["Default"])
        XCTAssertEqual(catalog.selectedId, defaultCoordinator.id)
        XCTAssertEqual(persistedSelections, [defaultCoordinator.id])
        XCTAssertFalse(catalog.isRefreshingLocals)
    }

    func testFailedRefreshPreservesLastKnownLocalSessions() async {
        let defaultCoordinator = ExtraCoordinator(
            id: "local:default",
            kind: .local,
            name: "eva-00",
            running: true,
            socketPath: "/tmp/default.sock",
            target: nil,
            session: "default"
        )
        let qaCoordinator = ExtraCoordinator(
            id: "local:release-qa",
            kind: .local,
            name: "release-qa",
            running: false,
            socketPath: "/tmp/release-qa.sock",
            target: nil,
            session: "release-qa"
        )
        var localCoordinators: [ExtraCoordinator]? = [defaultCoordinator, qaCoordinator]
        var persistedSelections: [String] = []
        let catalog = CoordinatorCatalog(
            localLoader: { localCoordinators },
            remoteRecords: [],
            selectedId: qaCoordinator.id,
            persistSelection: { persistedSelections.append($0) }
        )
        await catalog.refreshLocals()

        localCoordinators = nil
        await catalog.refreshLocals()

        XCTAssertEqual(catalog.coordinators.map(\.title), ["Default", "release-qa"])
        XCTAssertEqual(catalog.selectedId, qaCoordinator.id)
        XCTAssertEqual(persistedSelections, [])
        XCTAssertFalse(catalog.isRefreshingLocals)
    }
}

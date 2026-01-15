use kittycad_modeling_cmds as kcmc;

pub(crate) fn front() -> kcmc::ModelingCmd {
    kcmc::ModelingCmd::DefaultCameraLookAt(
        kcmc::DefaultCameraLookAt::builder()
            .up(kcmc::shared::Point3d { x: 0.0, y: 0.0, z: 1.0 })
            .vantage(kcmc::shared::Point3d {
                x: 0.0,
                y: -1.0,
                z: 0.0,
            })
            .center(kcmc::shared::Point3d { x: 0.0, y: 0.0, z: 0.0 })
            .maybe_sequence(None)
            .build(),
    )
}

pub(crate) fn right_side() -> kcmc::ModelingCmd {
    kcmc::ModelingCmd::DefaultCameraLookAt(
        kcmc::DefaultCameraLookAt::builder()
            .up(kcmc::shared::Point3d { x: 0.0, y: 0.0, z: 1.0 })
            .vantage(kcmc::shared::Point3d { x: 1.0, y: 0.0, z: 0.0 })
            .center(kcmc::shared::Point3d { x: 0.0, y: 0.0, z: 0.0 })
            .maybe_sequence(None)
            .build(),
    )
}

pub(crate) fn top() -> kcmc::ModelingCmd {
    kcmc::ModelingCmd::DefaultCameraLookAt(
        kcmc::DefaultCameraLookAt::builder()
            .up(kcmc::shared::Point3d { x: 0.0, y: 1.0, z: 0.0 })
            .vantage(kcmc::shared::Point3d { x: 0.0, y: 0.0, z: 1.0 })
            .center(kcmc::shared::Point3d { x: 0.0, y: 0.0, z: 0.0 })
            .maybe_sequence(None)
            .build(),
    )
}

pub(crate) fn iso() -> kcmc::ModelingCmd {
    kcmc::ModelingCmd::ViewIsometric(kcmc::ViewIsometric::builder().padding(0.0).build())
}

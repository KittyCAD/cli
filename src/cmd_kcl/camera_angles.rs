use kittycad_modeling_cmds as kcmc;

pub(crate) const FRONT: kcmc::ModelingCmd = kcmc::ModelingCmd::DefaultCameraLookAt(kcmc::DefaultCameraLookAt {
    up: kcmc::shared::Point3d { x: 0.0, y: 0.0, z: 1.0 },
    vantage: kcmc::shared::Point3d {
        x: 0.0,
        y: -1.0,
        z: 0.0,
    },
    center: kcmc::shared::Point3d { x: 0.0, y: 0.0, z: 0.0 },
    sequence: None,
});

pub(crate) const SIDE: kcmc::ModelingCmd = kcmc::ModelingCmd::DefaultCameraLookAt(kcmc::DefaultCameraLookAt {
    up: kcmc::shared::Point3d { x: 0.0, y: 0.0, z: 1.0 },
    vantage: kcmc::shared::Point3d { x: 1.0, y: 0.0, z: 0.0 },
    center: kcmc::shared::Point3d { x: 0.0, y: 0.0, z: 0.0 },
    sequence: None,
});

pub(crate) const TOP: kcmc::ModelingCmd = kcmc::ModelingCmd::DefaultCameraLookAt(kcmc::DefaultCameraLookAt {
    up: kcmc::shared::Point3d { x: 0.0, y: 1.0, z: 0.0 },
    vantage: kcmc::shared::Point3d { x: 0.0, y: 0.0, z: 1.0 },
    center: kcmc::shared::Point3d { x: 0.0, y: 0.0, z: 0.0 },
    sequence: None,
});

pub(crate) const ISO: kcmc::ModelingCmd = kcmc::ModelingCmd::ViewIsometric(kcmc::ViewIsometric { padding: 0.0 });

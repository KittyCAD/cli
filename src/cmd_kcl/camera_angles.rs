use kcmc::shared::Point3d;
use kittycad_modeling_cmds as kcmc;

const Z_UP: Point3d = Point3d { x: 0.0, y: 0.0, z: 1.0 };
const ZERO: Point3d = Point3d { x: 0.0, y: 0.0, z: 0.0 };

pub(crate) fn front() -> kcmc::ModelingCmd {
    kcmc::ModelingCmd::DefaultCameraLookAt(
        kcmc::DefaultCameraLookAt::builder()
            .up(Z_UP)
            .vantage(Point3d::only_y(-1.0))
            .center(ZERO)
            .build(),
    )
}

pub(crate) fn right_side() -> kcmc::ModelingCmd {
    kcmc::ModelingCmd::DefaultCameraLookAt(
        kcmc::DefaultCameraLookAt::builder()
            .up(Z_UP)
            .vantage(Point3d::only_x(1.0))
            .center(ZERO)
            .build(),
    )
}

pub(crate) fn top() -> kcmc::ModelingCmd {
    kcmc::ModelingCmd::DefaultCameraLookAt(
        kcmc::DefaultCameraLookAt::builder()
            .up(Point3d::only_y(1.0))
            .vantage(Z_UP)
            .center(ZERO)
            .build(),
    )
}

pub(crate) fn iso() -> kcmc::ModelingCmd {
    kcmc::ModelingCmd::ViewIsometric(kcmc::ViewIsometric::builder().padding(0.0).build())
}

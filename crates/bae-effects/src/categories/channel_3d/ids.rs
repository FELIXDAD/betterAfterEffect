use super::*;
use bae_model::ParamKind;
use settings::{CryptoSelection, IdChannel, Routing};

pub(super) fn entries() -> [Builtin; 4] {
    let mut matte = entry(
        DefBuilder::new("ADBE ID MATTE", "ID Matte", "3D Channel")
            .popup(
                1,
                "Aux. Channel",
                "辅助通道",
                2,
                &["Material ID", "Object ID"],
            )
            .static_param()
            .int(2, "ID Selection", "ID 选择", 0, [0, 32000])
            .slider(
                3,
                "Feather",
                "羽化",
                0.,
                [0., 32000.],
                [0., 100.],
                1,
                "pixels",
            )
            .checkbox(4, "Invert", "反转", false)
            .checkbox(5, "Use Coverage", "使用范围", false),
        Method::IdMatte,
        "id-matte",
        "ID 遮罩",
    );
    matte
        .info
        .param_options_zh
        .push((1, vec!["材质 ID".into(), "对象 ID".into()]));
    let custom =
        |b: &mut DefBuilder, id, name| b.push(id, name, name, ParamKind::ArbitraryData, None);
    let mut crypto = DefBuilder::new("Cryptomatte", "Cryptomatte", "3D Channel");
    custom(&mut crypto, 1, "Settings");
    let crypto = entry(
        crypto
            .popup(
                2,
                "Output",
                "Output",
                1,
                &["Colors", "Matted Colors", "Matted RGBA", "Matte Only"],
            )
            .checkbox(3, "Selection Mode", "Selection Mode", false),
        Method::Crypto,
        "cryptomatte",
        "Cryptomatte",
    );
    let mut extract = DefBuilder::new("EXtractoR", "EXtractoR", "3D Channel");
    custom(&mut extract, 13, "Channel Info");
    extract.push(11, "Process", "Process", ParamKind::GroupStart, None);
    let range = [-3.40282346638529e38, 3.40282346638529e38];
    extract = extract
        .slider(
            5,
            "Black Point",
            "Black Point",
            0.,
            range,
            [0., 1.],
            3,
            "none",
        )
        .slider(
            6,
            "White Point",
            "White Point",
            1.,
            range,
            [0., 1.],
            3,
            "none",
        )
        .checkbox(7, "UnMult", "UnMult", false);
    extract.push(12, "", "", ParamKind::GroupEnd, None);
    let extract = entry(extract, Method::Routing, "extractor", "EXtractoR");
    let mut id = DefBuilder::new("IDentifier", "IDentifier", "3D Channel");
    custom(&mut id, 5, "Channel Info");
    let id = entry(
        id.popup(
            3,
            "Display",
            "Display",
            1,
            &["Colors", "Luma Matte", "Alpha Matte", "Raw"],
        )
        .int(4, "ID", "ID", 0, [0, 2147483647]),
        Method::Identifier,
        "identifier",
        "IDentifier",
    );
    [matte, crypto, extract, id]
}

pub(super) fn uniforms(
    method: Method,
    channels: &FootageChannels,
    params: Params<'_>,
    u: &mut gpu::UniformWriter,
) {
    let mut ids = [-1; 4];
    match method {
        Method::IdMatte => {
            ids[0] = plane(
                channels,
                if params.popup(1) == 1 {
                    depth::MATERIAL
                } else {
                    depth::OBJECT
                },
            );
            ids[1] = plane(channels, &["Coverage", "Cov"]);
            u.u32(256, params.f(2) as u32);
        }
        Method::Identifier => {
            let choice: IdChannel = settings::read(params.value(5));
            ids[0] = if choice.name.is_empty() {
                plane(channels, depth::OBJECT)
            } else {
                plane(channels, &[&choice.name])
            };
            u.u32(256, params.f(4) as u32);
        }
        Method::Routing => {
            let routing: Routing = settings::read(params.value(13));
            ids = routing.channels.each_ref().map(|name| {
                if name.is_empty() {
                    -1
                } else {
                    plane(channels, &[name])
                }
            });
        }
        Method::Crypto => {
            let sel: CryptoSelection = settings::read(params.value(1));
            let groups = &channels.info.crypto;
            let layer = if sel.layer.is_empty() {
                groups.first()
            } else {
                groups.iter().find(|l| l.name == sel.layer)
            };
            if let Some(layer) = layer {
                u.u32(256, sel.object.unwrap_or_default())
                    .u32(260, layer.ranks.len() as u32);
                for (i, rank) in layer.ranks.iter().enumerate() {
                    u.i32s(272 + i * 16, rank);
                }
            }
        }
        _ => return,
    }
    u.i32s(80, &ids);
}

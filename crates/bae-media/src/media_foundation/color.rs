//! Media Foundation's video color enums use different numbers than H.273.

use windows::Win32::Media::MediaFoundation::*;
use windows::core::GUID;

use crate::ColorInfo;

const PRIMARIES: &[(i32, u8)] = &[
    (MFVideoPrimaries_BT709.0, 1),
    (MFVideoPrimaries_BT470_2_SysM.0, 4),
    (MFVideoPrimaries_BT470_2_SysBG.0, 5),
    (MFVideoPrimaries_SMPTE170M.0, 6),
    (MFVideoPrimaries_SMPTE_C.0, 6),
    (MFVideoPrimaries_BT2020.0, 9),
    (MFVideoPrimaries_XYZ.0, 10),
    (MFVideoPrimaries_DCI_P3.0, 11),
];
const TRANSFER: &[(i32, u8)] = &[
    (MFVideoTransFunc_709.0, 1),
    (MFVideoTransFunc_22.0, 4),
    (MFVideoTransFunc_28.0, 5),
    (MFVideoTransFunc_10.0, 8),
    (MFVideoTransFunc_sRGB.0, 13),
    (MFVideoTransFunc_2020.0, 14),
    (MFVideoTransFunc_2084.0, 16),
    (MFVideoTransFunc_HLG.0, 18),
];
const MATRIX: &[(i32, u8)] = &[
    (MFVideoTransferMatrix_BT709.0, 1),
    (MFVideoTransferMatrix_BT601.0, 6),
    (MFVideoTransferMatrix_BT2020_10.0, 9),
    (MFVideoTransferMatrix_BT2020_12.0, 9),
];

pub(super) fn read(ty: &IMFMediaType) -> ColorInfo {
    // SAFETY: optional UINT32 attributes on a live media type.
    let attr = |key: &GUID| unsafe { ty.GetUINT32(key).unwrap_or(0) as i32 };
    let cicp = |table: &[(i32, u8)], key| {
        let value = attr(key);
        table.iter().find(|p| p.0 == value).map_or(2, |p| p.1)
    };
    ColorInfo::from_cicp(
        cicp(PRIMARIES, &MF_MT_VIDEO_PRIMARIES),
        cicp(TRANSFER, &MF_MT_TRANSFER_FUNCTION),
        cicp(MATRIX, &MF_MT_YUV_MATRIX),
        attr(&MF_MT_VIDEO_NOMINAL_RANGE) == MFNominalRange_0_255.0,
    )
}

pub(super) fn write(ty: &IMFMediaType, color: &ColorInfo) -> windows::core::Result<()> {
    for (key, table, code) in [
        (MF_MT_VIDEO_PRIMARIES, PRIMARIES, color.primaries.to_cicp()),
        (MF_MT_TRANSFER_FUNCTION, TRANSFER, color.transfer.to_cicp()),
        (MF_MT_YUV_MATRIX, MATRIX, color.matrix.to_cicp()),
    ] {
        if let Some(&(value, _)) = table.iter().find(|p| p.1 == code) {
            // SAFETY: the selected key accepts the corresponding MF enum.
            unsafe { ty.SetUINT32(&key, value as u32)? };
        }
    }
    Ok(())
}

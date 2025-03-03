#![deny(missing_docs)]

/// Game Login packet.
pub mod game_login;
/// NC Login packet.
pub mod nc_login;
/// NC query packet.
pub mod nc_query;
/// NC Add weapon
pub mod nc_weapon_add;
/// NC weapon get packet.
pub mod nc_weapon_get;
/// RC Chat packet.
pub mod rc_chat;
/// RC Login packet.
pub mod rc_login;

use crate::define_packets;

define_packets! {
    FromClientPacketId {
        LevelWarp = 0x0,
        BoardModify = 0x1,
        PlayerProps = 0x2,
        // TODO: This could be NpcProps
        V4Login = 0x3,
        BombAdd = 0x4,
        V6Login = 0x5,
        V5Login = 0x6,
        HorseAdd = 0x7,
        HorseDel = 0x8,
        ArrowAdd = 0x9,
        FireSpy = 0xA,
        ThrowCarried = 0xB,
        ItemAdd = 0xC,
        ItemDel = 0xD,
        ClaimPker = 0xE,
        BaddyProps = 0xF,
        BaddyHurt = 0x10,
        BaddyAdd = 0x11,
        FlagSet = 0x12,
        FlagDel = 0x13,
        OpenChest = 0x14,
        PutNpc = 0x15,
        NpcDel = 0x16,
        WantFile = 0x17,
        ShowImg = 0x18,
        HurtPlayer = 0x1A,
        Explosion = 0x1B,
        PrivateMessage = 0x1C,
        NpcWeaponDel = 0x1D,
        LevelWarpMod = 0x1E,
        PacketCount = 0x1F,
        ItemTake = 0x20,
        WeaponAdd = 0x21,
        UpdateFile = 0x22,
        AdjacentLevel = 0x23,
        HitObjects = 0x24,
        Lang = 0x25,
        TriggerAction = 0x26,
        MapInfo = 0x27,
        Shoot = 0x28,
        ServerWarp = 0x29,
        MutePlayer = 0x2B,
        ProcessList = 0x2C,
        VerifyWantSend = 0x2F,
        Shoot2 = 0x30,
        RawData = 0x32,
        RcServerOptionsGet = 0x33,
        RcServerOptionsSet = 0x34,
        RcFolderConfigGet = 0x35,
        RcFolderConfigSet = 0x36,
        RcRespawnSet = 0x37,
        RcHorseLifeSet = 0x38,
        RcApIncrementSet = 0x39,
        RcBaddyRespawnSet = 0x3A,
        RcPlayerPropsGet = 0x3B,
        RcPlayerPropsSet = 0x3C,
        RcDisconnectPlayer = 0x3D,
        RcUpdateLevels = 0x3E,
        RcAdminMessage = 0x3F,
        RcPrivAdminMessage = 0x40,
        RcListRcs = 0x41,
        RcDisconnectRc = 0x42,
        RcApplyReason = 0x43,
        RcServerFlagsGet = 0x44,
        RcServerFlagsSet = 0x45,
        RcAccountAdd = 0x46,
        RcAccountDel = 0x47,
        RcAccountListGet = 0x48,
        RcPlayerPropsGet2 = 0x49,
        RcPlayerPropsGet3 = 0x4A,
        RcPlayerPropsReset = 0x4B,
        RcPlayerPropsSet2 = 0x4C,
        RcAccountGet = 0x4D,
        RcAccountSet = 0x4E,
        RcChat = 0x4F,
        ProfileGet = 0x50,
        ProfileSet = 0x51,
        RcWarpPlayer = 0x52,
        RcPlayerRightsGet = 0x53,
        RcPlayerRightsSet = 0x54,
        RcPlayerCommentsGet = 0x55,
        RcPlayerCommentsSet = 0x56,
        RcPlayerBanGet = 0x57,
        RcPlayerBanSet = 0x58,
        RcFileBrowserStart = 0x59,
        RcFileBrowserCd = 0x5A,
        RcFileBrowserEnd = 0x5B,
        RcFileBrowserDown = 0x5C,
        RcFileBrowserUp = 0x5D,
        NpcServerQuery = 0x5E,
        RcFileBrowserMove = 0x60,
        RcFileBrowserDelete = 0x61,
        RcFileBrowserRename = 0x62,
        NcNpcGet = 0x67,
        NcNpcDelete = 0x68,
        NcNpcReset = 0x69,
        NcNpcScriptGet = 0x6A,
        NcNpcWarp = 0x6B,
        NcNpcFlagsGet = 0x6C,
        NcNpcScriptSet = 0x6D,
        NcNpcFlagsSet = 0x6E,
        NcNpcAdd = 0x6F,
        NcClassEdit = 0x70,
        NcClassAdd = 0x71,
        NcLocalNpcsGet = 0x72,
        NcWeaponListGet = 0x73,
        NcWeaponGet = 0x74,
        NcWeaponAdd = 0x75,
        NcWeaponDelete = 0x76,
        NcClassDelete = 0x77,
        RequestUpdateBoard = 0x82,
        NcLevelListGet = 0x96,
        NcLevelListSet = 0x97,
        RequestText = 0x98,
        SendText = 0x9A,
        RcLargeFileStart = 0x9B,
        RcLargeFileEnd = 0x9C,
        UpdateGani = 0x9D,
        UpdateScript = 0x9E,
        UpdatePackageRequestFile = 0x9F,
        RcFolderDelete = 0xA0,
        UpdateClass = 0xA1,
        SetEncKey = 0xFC,
        Bundle = 0xFD
    }
}

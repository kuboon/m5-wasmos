//! 電池。ホストは電圧と電流だけを返すので、残量や電力はここで出す
//! （ここを直すのはアプリの更新だけで済む）。

pub use crate::bindings::wasmos::os::system::BatteryStatus as Battery;

/// 電池の状態（電池が無い・読めない・シミュレータなら None）
pub fn battery() -> Option<Battery> {
    crate::system::battery()
}

/// 充電・放電の別。±10mA 未満は止まっているとみなす
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Charging,
    Discharging,
    Idle,
}

/// リチウムイオン 1 セルの開放電圧（mV）と残量（%）のおおよその対応
const OCV: [(u32, u8); 11] = [
    (3300, 0),
    (3500, 5),
    (3600, 12),
    (3650, 20),
    (3700, 30),
    (3750, 40),
    (3800, 50),
    (3900, 65),
    (4000, 80),
    (4100, 90),
    (4200, 100),
];

impl Battery {
    /// 残量の目安（0..=100）。電圧からの推定なので、充電中は高め・負荷が重いと低めに出る
    pub fn level(&self) -> u8 {
        let mv = self.voltage_mv / self.cells.max(1) as u32;
        let (first, last) = (OCV[0], OCV[OCV.len() - 1]);
        if mv <= first.0 {
            return first.1;
        }
        if mv >= last.0 {
            return last.1;
        }
        let i = OCV.iter().position(|&(v, _)| v > mv).unwrap();
        let ((v0, p0), (v1, p1)) = (OCV[i - 1], OCV[i]);
        (p0 as u32 + (mv - v0) * (p1 - p0) as u32 / (v1 - v0)) as u8
    }

    /// 電池に入る電力（mW）。+ が充電、- が放電
    pub fn power_mw(&self) -> i32 {
        (self.voltage_mv as i64 * self.current_ma as i64 / 1000) as i32
    }

    pub fn flow(&self) -> Flow {
        match self.current_ma {
            c if c > 10 => Flow::Charging,
            c if c < -10 => Flow::Discharging,
            _ => Flow::Idle,
        }
    }
}

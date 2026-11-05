//! Phần dùng chung của trò chơi đua xe UDP: hằng số, vật lý, giả lập mất gói, bot, vẽ khung.
//!
//! Đơn vị: vị trí x và vận tốc v là số nguyên, tính bằng 1/100 ô (đường đua dài 40 ô = 4000).
//! Dùng số nguyên để server và client mọi ngôn ngữ tính ra CÙNG một kết quả (không sai số dấu phẩy động).
//!
//! Luật chơi: mỗi tick (50 ms) xe nhận một đầu vào: 1 = ga, 0 = nhả ga, -1 = phanh.
//! Đoạn đường từ ô 20 đến ô 26 là khúc cua: đi vào khúc cua nhanh hơn 0,50 ô/tick thì xe bị văng (tụt tốc độ).

pub const TICK_MS: u64 = 50; // server mô phỏng 20 tick mỗi giây
pub const START: i64 = 20; // đếm ngược 20 tick (1 s) trước khi xuất phát
pub const TRACK: i64 = 4000; // vạch đích
pub const ZONE_START: i64 = 2000; // khúc cua: [ZONE_START, ZONE_END)
pub const ZONE_END: i64 = 2600;
pub const V_CUA: i64 = 50; // tốc độ tối đa an toàn trong khúc cua
const V_VANG: i64 = 10; // tốc độ còn lại sau khi văng
const ACC: i64 = 6;
const DRAG: i64 = 2;
const BRAKE: i64 = 12;
const VMAX: i64 = 90;
pub const MAX_RACE: i64 = 600; // giới hạn 30 s
pub const LEAD: i64 = 2; // client gửi đầu vào cho tick k+2 khi nhận ảnh chụp tick k
pub const WINDOW: i64 = 3; // mỗi gói đầu vào mang kèm 3 đầu vào gần nhất (dư thừa chống mất gói)
pub const END_TICKS: i64 = 10; // sau khi đua xong, server gửi thêm 10 ảnh chụp kết quả rồi mới dừng

pub fn input_text(i: i64) -> &'static str {
    match i {
        1 => "ga",
        0 => "nhả",
        _ => "phanh",
    }
}

/// Một tick vật lý; trả (x, v, có_văng). Server và client dùng chung hàm này.
pub fn step(mut x: i64, mut v: i64, inp: i64) -> (i64, i64, bool) {
    v = if inp > 0 {
        (v + ACC).min(VMAX)
    } else if inp < 0 {
        (v - BRAKE).max(0)
    } else {
        (v - DRAG).max(0)
    };
    x += v;
    let crashed = (ZONE_START..ZONE_END).contains(&x) && v > V_CUA;
    if crashed {
        v = V_VANG;
    }
    (x, v, crashed)
}

/// Giả lập mất gói: true nếu gói (hướng, xe, số thứ tự) bị "mất".
/// Quyết định là hàm băm của các tham số, không phụ thuộc thời điểm, nên cùng seed cho cùng các gói
/// bị mất ở mọi lần chạy và ở mọi bản ngôn ngữ (phép tính lấy mod 2^32 như bản Python).
pub fn lossy(seed: i64, direction: i64, car: i64, key: i64, pct: i64) -> bool {
    if pct <= 0 {
        return false;
    }
    let mut h = seed
        .wrapping_mul(2654435761)
        .wrapping_add(direction.wrapping_mul(40503))
        .wrapping_add(car.wrapping_mul(9973))
        .wrapping_add(key.wrapping_mul(2246822519)) as u64
        & 0xFFFF_FFFF;
    h ^= h >> 16;
    h = (h * 0x45D9F3B) & 0xFFFF_FFFF;
    h ^= h >> 16;
    h = (h * 0x45D9F3B) & 0xFFFF_FFFF;
    h ^= h >> 16;
    (h % 100) < pct as u64
}

// ---------------------------------------------------------------- bot
/// (khoảng an toàn trước khúc cua, tốc độ trong cua) của hai kiểu bot.
pub fn style(name: &str) -> Option<(i64, i64)> {
    match name {
        "lieu" => Some((0, V_CUA)),
        "can_than" => Some((300, V_CUA - 10)),
        _ => None,
    }
}

/// Nếu tick này dùng inp rồi phanh liên tục, xe có vào khúc cua quá nhanh không?
fn would_crash(mut x: i64, v: i64, inp: i64, zone_start: i64, limit: i64) -> bool {
    let mut v = match inp {
        1 => (v + ACC).min(VMAX),
        0 => (v - DRAG).max(0),
        _ => (v - BRAKE).max(0),
    };
    x += v;
    loop {
        if x >= ZONE_END {
            return false;
        }
        if x >= zone_start && v > limit {
            return true;
        }
        if v <= limit {
            return false;
        }
        v = (v - BRAKE).max(0);
        x += v;
    }
}

/// Chọn đầu vào: ga nhiều nhất có thể mà vẫn kịp phanh trước khúc cua.
pub fn bot_input(x: i64, v: i64, (safety, limit): (i64, i64)) -> i64 {
    let zone_start = ZONE_START - safety;
    if (zone_start..ZONE_END).contains(&x) {
        // đang ở khúc cua (hoặc vùng an toàn trước nó)
        if v + ACC <= limit {
            return 1;
        }
        return if v - DRAG <= limit { 0 } else { -1 };
    }
    for inp in [1, 0, -1] {
        if !would_crash(x, v, inp, zone_start, limit) {
            return inp;
        }
    }
    -1
}

// ---------------------------------------------------------------- hiển thị
/// Số nguyên 1/100 ô -> chuỗi có hai chữ số thập phân (ví dụ 1234 -> "12.34").
pub fn fmt2(n: i64) -> String {
    format!("{}.{:02}", n.div_euclid(100), n.rem_euclid(100))
}

/// Vẽ một làn đua 40 ký tự: '~' là khúc cua, '|' là vạch đích.
pub fn track_line(letter: char, x: i64) -> String {
    let pos = 39.min(x.div_euclid(100));
    let mut s: String = (0..40)
        .map(|i| {
            if i == pos {
                letter
            } else if (ZONE_START / 100..ZONE_END / 100).contains(&i) {
                '~'
            } else {
                '.'
            }
        })
        .collect();
    s.push('|');
    s
}

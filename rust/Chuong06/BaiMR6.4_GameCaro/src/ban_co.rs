//! Bàn cờ Caro 15×15: kiểm tra thắng (5 quân liên tiếp), hiển thị, bot đơn giản.
//!
//! Server dùng five_line() để PHÁN XỬ (server là bên có thẩm quyền); client dùng render() để vẽ bàn cờ
//! và choose_move() khi chạy ở chế độ bot. Bot là hàm tất định, cùng công thức với bản Python và Go
//! nên các bản cho cùng ván đấu.

pub const N: usize = 15; // kích thước bàn cờ
pub const EMPTY: u8 = b'.';
const DIRS: [(i32, i32); 4] = [(0, 1), (1, 0), (1, 1), (1, -1)]; // ngang, dọc, chéo xuống, chéo lên

pub type Board = Vec<Vec<u8>>;

pub fn new_board() -> Board {
    vec![vec![EMPTY; N]; N]
}

pub fn inside(r: i32, c: i32) -> bool {
    (0..N as i32).contains(&r) && (0..N as i32).contains(&c)
}

/// (7, 7) -> "h8": cột là chữ a–o, hàng là số 1–15.
pub fn cell_name(r: i64, c: i64) -> String {
    format!("{}{}", (b'a' + c as u8) as char, r + 1)
}

/// "h8" -> (7, 7); None nếu sai cú pháp hoặc ngoài bàn cờ.
pub fn parse_cell(text: &str) -> Option<(usize, usize)> {
    let t = text.trim().to_lowercase();
    let mut ch = t.chars();
    let col = ch.next()?;
    let rest: String = ch.collect();
    if !('a'..='o').contains(&col) || rest.is_empty() || !rest.chars().all(|d| d.is_ascii_digit()) {
        return None;
    }
    let r = rest.parse::<i32>().ok()? - 1;
    let c = col as i32 - 'a' as i32;
    inside(r, c).then_some((r as usize, c as usize))
}

/// Nếu quân vừa đặt ở (r, c) tạo thành dãy ≥ 5 quân cùng màu thì trả danh sách ô của dãy (đã sắp xếp).
pub fn five_line(b: &Board, r: usize, c: usize) -> Option<Vec<(usize, usize)>> {
    let mark = b[r][c];
    for (dr, dc) in DIRS {
        let mut line = vec![(r, c)];
        for sgn in [1, -1] {
            // đi về hai phía của ô vừa đặt
            let (mut rr, mut cc) = (r as i32 + dr * sgn, c as i32 + dc * sgn);
            while inside(rr, cc) && b[rr as usize][cc as usize] == mark {
                line.push((rr as usize, cc as usize));
                rr += dr * sgn;
                cc += dc * sgn;
            }
        }
        if line.len() >= 5 {
            line.sort();
            return Some(line);
        }
    }
    None
}

/// Vẽ bàn cờ thành các dòng chữ; ô vừa đánh (last) được đánh dấu bằng chữ thường x/o.
pub fn render(b: &Board, last: Option<(usize, usize)>) -> Vec<String> {
    let cols: Vec<String> = (0..N).map(|c| ((b'a' + c as u8) as char).to_string()).collect();
    let mut lines = vec![format!("    {}", cols.join(" "))];
    for r in 0..N {
        let cells: Vec<String> = (0..N)
            .map(|c| {
                let v = b[r][c] as char;
                if Some((r, c)) == last { v.to_ascii_lowercase().to_string() } else { v.to_string() }
            })
            .collect();
        lines.push(format!("{:>3} {}", r + 1, cells.join(" ")));
    }
    lines
}

// ---------------------------------------------------------------- bot
/// Giá trị của một hướng theo (độ dài dãy, số đầu còn trống): dãy 4 hở hai đầu gần như chắc thắng.
fn value(run: i32, ends: i32) -> i64 {
    match (run, ends) {
        (4, 2) => 100000,
        (4, 1) => 10000,
        (3, 2) => 5000,
        (3, 1) => 500,
        (2, 2) => 200,
        (2, 1) => 50,
        (1, 2) => 10,
        (1, 1) => 2,
        _ => 0,
    }
}

/// Kiểu bot: (hệ số tấn công, hệ số phòng thủ, độ "nhiễu"). Bot tập sự cộng thêm điểm ngẫu nhiên nên hay
/// bỏ sót thế cờ nguy hiểm (nhưng vẫn luôn thắng ngay khi có thể và luôn chặn dãy 4).
pub fn style(name: &str) -> Option<(i64, i64, i64)> {
    match name {
        "tan_cong" => Some((10, 7, 0)),
        "phong_thu" => Some((7, 10, 0)),
        "can_bang" => Some((10, 9, 0)),
        "tap_su" => Some((10, 9, 50000)),
        _ => None,
    }
}

/// Bộ sinh số giả ngẫu nhiên LCG (cùng công thức với MR3.3, bản Python và Go) – để ván đấu tái lập được.
pub struct Lcg {
    x: u64,
}

impl Lcg {
    pub fn new(seed: i64) -> Lcg {
        Lcg { x: (seed as u64) & 0x7FFF_FFFF }
    }
    pub fn next_int(&mut self, lo: i64, hi: i64) -> i64 {
        self.x = (self.x * 1103515245 + 12345) & 0x7FFF_FFFF;
        lo + (self.x >> 8) as i64 % (hi - lo + 1)
    }
}

/// Giá trị của việc đặt quân mark vào ô trống (r, c); trả -1 nếu tạo được 5 quân (thắng ngay).
fn line_value(b: &Board, r: usize, c: usize, mark: u8) -> i64 {
    let mut total = 0;
    for (dr, dc) in DIRS {
        let (mut run, mut ends) = (1, 0);
        for sgn in [1, -1] {
            let (mut rr, mut cc) = (r as i32 + dr * sgn, c as i32 + dc * sgn);
            while inside(rr, cc) && b[rr as usize][cc as usize] == mark {
                run += 1;
                rr += dr * sgn;
                cc += dc * sgn;
            }
            if inside(rr, cc) && b[rr as usize][cc as usize] == EMPTY {
                ends += 1;
            }
        }
        if run >= 5 {
            return -1;
        }
        total += value(run, ends);
    }
    total
}

/// Ô có quân nào trong phạm vi 2 ô không (chỉ xét các ô này cho nhanh).
fn near_stone(b: &Board, r: usize, c: usize) -> bool {
    (r.saturating_sub(2)..(r + 3).min(N)).any(|rr| (c.saturating_sub(2)..(c + 3).min(N)).any(|cc| b[rr][cc] != EMPTY))
}

/// Chọn nước đi: thắng ngay nếu được, chặn nếu đối thủ sắp thắng, còn lại cộng điểm tấn công + phòng thủ.
pub fn choose_move(b: &Board, me: u8, st: (i64, i64, i64), rng: &mut Lcg) -> (usize, usize) {
    let opp = if me == b'X' { b'O' } else { b'X' };
    let (att, dfn, noise) = st;
    let (mut best, mut best_key) = ((N / 2, N / 2), None); // bàn trống: đánh vào giữa
    for r in 0..N {
        for c in 0..N {
            if b[r][c] != EMPTY || !near_stone(b, r, c) {
                continue;
            }
            let (mine, theirs) = (line_value(b, r, c, me), line_value(b, r, c, opp));
            let score = if mine < 0 {
                1_000_000_000_000 // thắng ngay
            } else if theirs < 0 {
                100_000_000_000 // bắt buộc chặn
            } else {
                let mut s = att * mine + dfn * theirs;
                if noise > 0 {
                    s += rng.next_int(0, noise);
                }
                s
            };
            // hòa điểm: gần tâm, rồi theo hàng, cột
            let key = (-score, (r as i64 - 7).abs() + (c as i64 - 7).abs(), r, c);
            if best_key.is_none_or(|k| key < k) {
                best = (r, c);
                best_key = Some(key);
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nam_quan_lien_tiep() {
        let mut b = new_board();
        for r in 3..8 {
            b[r][5] = b'X';
        }
        assert_eq!(five_line(&b, 5, 5).unwrap().len(), 5);
        assert_eq!(parse_cell("h8"), Some((7, 7)));
        assert_eq!(parse_cell("p1"), None);
        assert_eq!(cell_name(3, 5), "f4");
    }
}

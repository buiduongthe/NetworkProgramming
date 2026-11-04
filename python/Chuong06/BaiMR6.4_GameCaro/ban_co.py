"""Bài mở rộng MR6.4 – Bàn cờ Caro 15×15: kiểm tra thắng (5 quân liên tiếp), hiển thị, bot đơn giản.

Server dùng five_line() để PHÁN XỬ (server là bên có thẩm quyền); client dùng render() để vẽ bàn cờ
và choose_move() khi chạy ở chế độ bot. Bot là hàm tất định (không ngẫu nhiên) nên hai bản Python, Go
cho cùng ván đấu.
"""
N = 15                                   # kích thước bàn cờ
EMPTY = "."
DIRS = [(0, 1), (1, 0), (1, 1), (1, -1)]   # ngang, dọc, chéo xuống, chéo lên


def new_board():
    return [[EMPTY] * N for _ in range(N)]


def inside(r, c):
    return 0 <= r < N and 0 <= c < N


def cell_name(r, c):
    """(7, 7) -> "h8": cột là chữ a–o, hàng là số 1–15."""
    return f"{chr(ord('a') + c)}{r + 1}"


def parse_cell(text):
    """"h8" -> (7, 7); trả None nếu sai cú pháp hoặc ngoài bàn cờ."""
    text = text.strip().lower()
    if len(text) < 2 or not "a" <= text[0] <= "o" or not text[1:].isdigit():
        return None
    r, c = int(text[1:]) - 1, ord(text[0]) - ord("a")
    return (r, c) if inside(r, c) else None


def five_line(board, r, c):
    """Nếu quân vừa đặt ở (r, c) tạo thành dãy ≥ 5 quân cùng màu thì trả danh sách ô của dãy, ngược lại None."""
    mark = board[r][c]
    for dr, dc in DIRS:
        line = [(r, c)]
        for sgn in (1, -1):                                  # đi về hai phía của ô vừa đặt
            rr, cc = r + dr * sgn, c + dc * sgn
            while inside(rr, cc) and board[rr][cc] == mark:
                line.append((rr, cc))
                rr, cc = rr + dr * sgn, cc + dc * sgn
        if len(line) >= 5:
            return sorted(line)
    return None


def render(board, last=None):
    """Vẽ bàn cờ thành các dòng chữ; ô vừa đánh (last) được đánh dấu bằng chữ thường x/o."""
    lines = ["    " + " ".join(chr(ord("a") + c) for c in range(N))]
    for r in range(N):
        cells = []
        for c in range(N):
            v = board[r][c]
            cells.append(v.lower() if (r, c) == last else v)
        lines.append(f"{r + 1:>3} " + " ".join(cells))
    return lines


# ---------------------------------------------------------------- bot
# Giá trị của một hướng theo (độ dài dãy, số đầu còn trống): dãy 4 hở hai đầu gần như chắc thắng.
VALUE = {(4, 2): 100000, (4, 1): 10000, (3, 2): 5000, (3, 1): 500,
         (2, 2): 200, (2, 1): 50, (1, 2): 10, (1, 1): 2}
# Kiểu bot: (hệ số tấn công, hệ số phòng thủ, độ "nhiễu"). Bot tập sự cộng thêm điểm ngẫu nhiên nên hay bỏ sót
# thế cờ nguy hiểm (nhưng vẫn luôn thắng ngay khi có thể và luôn chặn dãy 4).
STYLES = {"tan_cong": (10, 7, 0), "phong_thu": (7, 10, 0), "can_bang": (10, 9, 0), "tap_su": (10, 9, 50000)}


class Lcg:
    """Bộ sinh số giả ngẫu nhiên LCG (cùng công thức với MR3.3 và bản Go) – để ván đấu của bot tái lập được."""

    def __init__(self, seed):
        self.x = seed & 0x7FFFFFFF

    def next_int(self, lo, hi):
        self.x = (self.x * 1103515245 + 12345) & 0x7FFFFFFF
        return lo + (self.x >> 8) % (hi - lo + 1)


def line_value(board, r, c, mark):
    """Giá trị của việc đặt quân mark vào ô trống (r, c); trả -1 nếu tạo được 5 quân (thắng ngay)."""
    total = 0
    for dr, dc in DIRS:
        run, ends = 1, 0
        for sgn in (1, -1):
            rr, cc = r + dr * sgn, c + dc * sgn
            while inside(rr, cc) and board[rr][cc] == mark:
                run += 1
                rr, cc = rr + dr * sgn, cc + dc * sgn
            if inside(rr, cc) and board[rr][cc] == EMPTY:
                ends += 1
        if run >= 5:
            return -1
        total += VALUE.get((run, ends), 0)
    return total


def near_stone(board, r, c):
    """Ô có quân nào trong phạm vi 2 ô không (chỉ xét các ô này cho nhanh)."""
    for rr in range(max(0, r - 2), min(N, r + 3)):
        for cc in range(max(0, c - 2), min(N, c + 3)):
            if board[rr][cc] != EMPTY:
                return True
    return False


def choose_move(board, me, style, rng):
    """Chọn nước đi: thắng ngay nếu được, chặn nếu đối thủ sắp thắng, còn lại cộng điểm tấn công + phòng thủ."""
    opp = "O" if me == "X" else "X"
    att, dfn, noise = STYLES[style]
    best, best_key = (N // 2, N // 2), None           # bàn trống: đánh vào giữa
    for r in range(N):
        for c in range(N):
            if board[r][c] != EMPTY or not near_stone(board, r, c):
                continue
            mine, theirs = line_value(board, r, c, me), line_value(board, r, c, opp)
            if mine < 0:
                score = 10 ** 12                      # thắng ngay
            elif theirs < 0:
                score = 10 ** 11                      # bắt buộc chặn
            else:
                score = att * mine + dfn * theirs
                if noise:
                    score += rng.next_int(0, noise)
            key = (-score, abs(r - N // 2) + abs(c - N // 2), r, c)   # hòa điểm: gần tâm, rồi theo hàng, cột
            if best_key is None or key < best_key:
                best, best_key = (r, c), key
    return best

"""Bài mở rộng MR4.2 – Phần dùng chung của trò chơi đua xe UDP: hằng số, vật lý, giả lập mất gói, bot, vẽ khung.

Đơn vị: vị trí x và vận tốc v là số nguyên, tính bằng 1/100 ô (đường đua dài 40 ô = 4000).
Dùng số nguyên để server, client Python và client Go tính ra CÙNG một kết quả (không sai số dấu phẩy động).

Luật chơi: mỗi tick (50 ms) xe nhận một đầu vào: 1 = ga, 0 = nhả ga, -1 = phanh.
Đoạn đường từ ô 20 đến ô 26 là khúc cua: đi vào khúc cua nhanh hơn 0,50 ô/tick thì xe bị văng (tụt tốc độ).
"""
TICK_MS = 50            # server mô phỏng 20 tick mỗi giây
START = 20              # đếm ngược 20 tick (1 s) trước khi xuất phát
TRACK = 4000            # vạch đích
ZONE_START, ZONE_END = 2000, 2600   # khúc cua
V_CUA = 50              # tốc độ tối đa an toàn trong khúc cua
V_VANG = 10             # tốc độ còn lại sau khi văng
ACC, DRAG, BRAKE, VMAX = 6, 2, 12, 90
MAX_RACE = 600          # giới hạn 30 s
LEAD = 2                # client gửi đầu vào cho tick k+2 khi nhận ảnh chụp tick k (bù trễ, có dư thời gian)
WINDOW = 3              # mỗi gói đầu vào mang kèm 3 đầu vào gần nhất (dư thừa chống mất gói)
END_TICKS = 10          # sau khi đua xong, server gửi thêm 10 ảnh chụp kết quả rồi mới dừng
INPUT_TEXT = {1: "ga", 0: "nhả", -1: "phanh"}


def step(x, v, inp):
    """Một tick vật lý; trả (x, v, có_văng). Server và client dùng chung hàm này."""
    if inp > 0:
        v = min(v + ACC, VMAX)
    elif inp < 0:
        v = max(v - BRAKE, 0)
    else:
        v = max(v - DRAG, 0)
    x += v
    crashed = ZONE_START <= x < ZONE_END and v > V_CUA
    if crashed:
        v = V_VANG
    return x, v, crashed


def lossy(seed, direction, car, key, pct):
    """Giả lập mất gói: True nếu gói (hướng, xe, số thứ tự) bị "mất".

    Quyết định là hàm băm của các tham số, không phụ thuộc thời điểm, nên cùng seed cho cùng các gói bị mất
    ở mọi lần chạy và ở cả hai bản Python, Go.
    """
    if pct <= 0:
        return False
    m = 0xFFFFFFFF
    h = (seed * 2654435761 + direction * 40503 + car * 9973 + key * 2246822519) & m
    h ^= h >> 16
    h = (h * 0x45D9F3B) & m
    h ^= h >> 16
    h = (h * 0x45D9F3B) & m
    h ^= h >> 16
    return h % 100 < pct


# ---------------------------------------------------------------- bot
STYLES = {"lieu": (0, V_CUA), "can_than": (300, V_CUA - 10)}   # (khoảng an toàn trước khúc cua, tốc độ trong cua)


def would_crash(x, v, inp, zone_start, limit):
    """Nếu tick này dùng inp rồi phanh liên tục, xe có vào khúc cua quá nhanh không?"""
    v = {1: min(v + ACC, VMAX), 0: max(v - DRAG, 0), -1: max(v - BRAKE, 0)}[inp]
    x += v
    while True:
        if x >= ZONE_END:
            return False
        if x >= zone_start and v > limit:
            return True
        if v <= limit:
            return False
        v = max(v - BRAKE, 0)
        x += v


def bot_input(x, v, style):
    """Chọn đầu vào: ga nhiều nhất có thể mà vẫn kịp phanh trước khúc cua."""
    safety, limit = STYLES[style]
    zone_start = ZONE_START - safety
    if zone_start <= x < ZONE_END:                 # đang ở khúc cua (hoặc vùng an toàn trước nó)
        if v + ACC <= limit:
            return 1
        return 0 if v - DRAG <= limit else -1
    for inp in (1, 0, -1):
        if not would_crash(x, v, inp, zone_start, limit):
            return inp
    return -1


# ---------------------------------------------------------------- hiển thị
def fmt2(n):
    """Số nguyên 1/100 ô -> chuỗi có hai chữ số thập phân (ví dụ 1234 -> "12.34")."""
    return f"{n // 100}.{n % 100:02d}"


def track_line(letter, x):
    """Vẽ một làn đua 40 ký tự: '~' là khúc cua, '|' là vạch đích."""
    pos = min(39, x // 100)
    cells = []
    for i in range(40):
        if i == pos:
            cells.append(letter)
        elif ZONE_START // 100 <= i < ZONE_END // 100:
            cells.append("~")
        else:
            cells.append(".")
    return "".join(cells) + "|"

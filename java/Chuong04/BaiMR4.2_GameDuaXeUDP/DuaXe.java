import java.util.Map;

/**
 * Bài mở rộng MR4.2 – Phần dùng chung của trò chơi đua xe UDP: hằng số, vật lý, giả lập mất gói, bot, vẽ khung.
 *
 * Đơn vị: vị trí x và vận tốc v là số nguyên, tính bằng 1/100 ô (đường đua dài 40 ô = 4000).
 * Dùng số nguyên để server và client mọi ngôn ngữ tính ra CÙNG một kết quả (không sai số dấu phẩy động).
 *
 * Luật chơi: mỗi tick (50 ms) xe nhận một đầu vào: 1 = ga, 0 = nhả ga, -1 = phanh.
 * Đoạn đường từ ô 20 đến ô 26 là khúc cua: đi vào khúc cua nhanh hơn 0,50 ô/tick thì xe bị văng (tụt tốc độ).
 */
final class DuaXe {
    private DuaXe() {}

    static final int TICK_MS = 50;            // server mô phỏng 20 tick mỗi giây
    static final int START = 20;              // đếm ngược 20 tick (1 s) trước khi xuất phát
    static final int TRACK = 4000;            // vạch đích
    static final int ZONE_START = 2000, ZONE_END = 2600;   // khúc cua
    static final int V_CUA = 50;              // tốc độ tối đa an toàn trong khúc cua
    static final int V_VANG = 10;             // tốc độ còn lại sau khi văng
    static final int ACC = 6, DRAG = 2, BRAKE = 12, VMAX = 90;
    static final int MAX_RACE = 600;          // giới hạn 30 s
    static final int LEAD = 2;                // client gửi đầu vào cho tick k+2 khi nhận ảnh chụp tick k
    static final int WINDOW = 3;              // mỗi gói đầu vào mang kèm 3 đầu vào gần nhất (dư thừa chống mất gói)
    static final int END_TICKS = 10;          // sau khi đua xong, server gửi thêm 10 ảnh chụp kết quả rồi mới dừng
    static final Map<Integer, String> INPUT_TEXT = Map.of(1, "ga", 0, "nhả", -1, "phanh");

    /** Kết quả một tick vật lý. */
    record Step(int x, int v, boolean crashed) {}

    /** Một tick vật lý. Server và client dùng chung hàm này. */
    static Step step(int x, int v, int inp) {
        if (inp > 0) v = Math.min(v + ACC, VMAX);
        else if (inp < 0) v = Math.max(v - BRAKE, 0);
        else v = Math.max(v - DRAG, 0);
        x += v;
        boolean crashed = ZONE_START <= x && x < ZONE_END && v > V_CUA;
        if (crashed) v = V_VANG;
        return new Step(x, v, crashed);
    }

    /**
     * Giả lập mất gói: true nếu gói (hướng, xe, số thứ tự) bị "mất".
     * Quyết định là hàm băm của các tham số, không phụ thuộc thời điểm, nên cùng seed cho cùng các gói bị mất
     * ở mọi lần chạy và ở mọi bản ngôn ngữ. Phép tính dùng long rồi giữ 32 bit thấp (& M) như uint32.
     */
    static boolean lossy(long seed, int direction, int car, long key, int pct) {
        if (pct <= 0) return false;
        final long M = 0xFFFFFFFFL;
        long h = (seed * 2654435761L + direction * 40503L + car * 9973L + key * 2246822519L) & M;
        h ^= h >>> 16;
        h = (h * 0x45D9F3BL) & M;
        h ^= h >>> 16;
        h = (h * 0x45D9F3BL) & M;
        h ^= h >>> 16;
        return h % 100 < pct;
    }

    // ---------------------------------------------------------------- bot
    /** Kiểu lái: khoảng an toàn trước khúc cua, tốc độ trong cua. */
    record Style(int safety, int limit) {}
    static final Map<String, Style> STYLES = Map.of("lieu", new Style(0, V_CUA), "can_than", new Style(300, V_CUA - 10));

    /** Nếu tick này dùng inp rồi phanh liên tục, xe có vào khúc cua quá nhanh không? */
    static boolean wouldCrash(int x, int v, int inp, int zoneStart, int limit) {
        v = inp == 1 ? Math.min(v + ACC, VMAX) : inp == 0 ? Math.max(v - DRAG, 0) : Math.max(v - BRAKE, 0);
        x += v;
        while (true) {
            if (x >= ZONE_END) return false;
            if (x >= zoneStart && v > limit) return true;
            if (v <= limit) return false;
            v = Math.max(v - BRAKE, 0);
            x += v;
        }
    }

    /** Chọn đầu vào: ga nhiều nhất có thể mà vẫn kịp phanh trước khúc cua. */
    static int botInput(int x, int v, String style) {
        Style s = STYLES.get(style);
        int zoneStart = ZONE_START - s.safety();
        if (zoneStart <= x && x < ZONE_END) {             // đang ở khúc cua (hoặc vùng an toàn trước nó)
            if (v + ACC <= s.limit()) return 1;
            return v - DRAG <= s.limit() ? 0 : -1;
        }
        for (int inp : new int[] {1, 0, -1})
            if (!wouldCrash(x, v, inp, zoneStart, s.limit())) return inp;
        return -1;
    }

    // ---------------------------------------------------------------- hiển thị
    /** Số nguyên 1/100 ô -> chuỗi có hai chữ số thập phân (ví dụ 1234 -> "12.34"). */
    static String fmt2(int n) { return (n / 100) + "." + String.format("%02d", n % 100); }

    /** Vẽ một làn đua 40 ký tự: '~' là khúc cua, '|' là vạch đích. */
    static String trackLine(char letter, int x) {
        int pos = Math.min(39, x / 100);
        StringBuilder b = new StringBuilder();
        for (int i = 0; i < 40; i++) {
            if (i == pos) b.append(letter);
            else if (ZONE_START / 100 <= i && i < ZONE_END / 100) b.append('~');
            else b.append('.');
        }
        return b.append('|').toString();
    }
}

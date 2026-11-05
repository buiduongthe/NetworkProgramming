import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Map;

/**
 * Bài mở rộng MR6.4 – Bàn cờ Caro 15×15: kiểm tra thắng (5 quân liên tiếp), hiển thị, bot đơn giản.
 *
 * Server dùng fiveLine() để PHÁN XỬ (server là bên có thẩm quyền); client dùng render() để vẽ bàn cờ và
 * chooseMove() khi chạy ở chế độ bot. Bot tất định (chỉ dùng bộ sinh LCG có hạt giống) nên các bản ngôn ngữ
 * cho cùng ván đấu.
 */
final class BanCo {
    static final int N = 15;                       // kích thước bàn cờ
    static final char EMPTY = '.';
    static final int[][] DIRS = {{0, 1}, {1, 0}, {1, 1}, {1, -1}};   // ngang, dọc, chéo xuống, chéo lên

    final char[][] o = new char[N][N];

    BanCo() { for (char[] row : o) Arrays.fill(row, EMPTY); }

    static boolean inside(int r, int c) { return r >= 0 && r < N && c >= 0 && c < N; }

    /** (7, 7) -> "h8": cột là chữ a–o, hàng là số 1–15. */
    static String cellName(int r, int c) { return (char) ('a' + c) + String.valueOf(r + 1); }

    /** "h8" -> {7, 7}; null nếu sai cú pháp hoặc ngoài bàn cờ. */
    static int[] parseCell(String text) {
        text = text.strip().toLowerCase(java.util.Locale.ROOT);
        if (text.length() < 2 || text.charAt(0) < 'a' || text.charAt(0) > 'o' || !text.substring(1).chars().allMatch(Character::isDigit))
            return null;
        int r = Integer.parseInt(text.substring(1)) - 1, c = text.charAt(0) - 'a';
        return inside(r, c) ? new int[] {r, c} : null;
    }

    /** Nếu quân vừa đặt ở (r, c) tạo thành dãy ≥ 5 quân cùng màu thì trả các ô của dãy, ngược lại null. */
    List<int[]> fiveLine(int r, int c) {
        char mark = o[r][c];
        for (int[] d : DIRS) {
            List<int[]> line = new ArrayList<>(List.of(new int[] {r, c}));
            for (int sgn : new int[] {1, -1}) {            // đi về hai phía của ô vừa đặt
                int rr = r + d[0] * sgn, cc = c + d[1] * sgn;
                while (inside(rr, cc) && o[rr][cc] == mark) {
                    line.add(new int[] {rr, cc});
                    rr += d[0] * sgn;
                    cc += d[1] * sgn;
                }
            }
            if (line.size() >= 5) {
                line.sort((a, b) -> a[0] != b[0] ? Integer.compare(a[0], b[0]) : Integer.compare(a[1], b[1]));
                return line;
            }
        }
        return null;
    }

    /** Vẽ bàn cờ thành các dòng chữ; ô vừa đánh (last) được đánh dấu bằng chữ thường x/o. */
    List<String> render(int lastR, int lastC) {
        List<String> lines = new ArrayList<>();
        StringBuilder head = new StringBuilder("   ");
        for (int c = 0; c < N; c++) head.append(' ').append((char) ('a' + c));
        lines.add(head.toString());
        for (int r = 0; r < N; r++) {
            StringBuilder b = new StringBuilder(String.format("%3d", r + 1));
            for (int c = 0; c < N; c++) {
                char v = o[r][c];
                if (r == lastR && c == lastC) v = Character.toLowerCase(v);
                b.append(' ').append(v);
            }
            lines.add(b.toString());
        }
        return lines;
    }

    // ---------------------------------------------------------------- bot

    /** Giá trị của một hướng theo (độ dài dãy, số đầu còn trống): dãy 4 hở hai đầu gần như chắc thắng. */
    static long value(int run, int ends) {
        return switch (run * 10 + ends) {
            case 42 -> 100000; case 41 -> 10000; case 32 -> 5000; case 31 -> 500;
            case 22 -> 200; case 21 -> 50; case 12 -> 10; case 11 -> 2;
            default -> 0;
        };
    }

    /**
     * Kiểu bot: (hệ số tấn công, hệ số phòng thủ, độ "nhiễu"). Bot tập sự cộng thêm điểm ngẫu nhiên nên hay bỏ sót
     * thế cờ nguy hiểm (nhưng vẫn luôn thắng ngay khi có thể và luôn chặn dãy 4).
     */
    static final Map<String, long[]> STYLES = Map.of("tan_cong", new long[] {10, 7, 0}, "phong_thu", new long[] {7, 10, 0},
            "can_bang", new long[] {10, 9, 0}, "tap_su", new long[] {10, 9, 50000});

    /** Bộ sinh số giả ngẫu nhiên LCG (cùng công thức với MR3.3 và các bản Python, Go). */
    static final class Lcg {
        long x;
        Lcg(long seed) { x = seed & 0x7FFFFFFFL; }
        int nextInt(int lo, int hi) {
            x = (x * 1103515245L + 12345) & 0x7FFFFFFFL;
            return lo + (int) ((x >> 8) % (hi - lo + 1));
        }
    }

    /** Giá trị của việc đặt quân mark vào ô trống (r, c); trả -1 nếu tạo được 5 quân (thắng ngay). */
    long lineValue(int r, int c, char mark) {
        long total = 0;
        for (int[] d : DIRS) {
            int run = 1, ends = 0;
            for (int sgn : new int[] {1, -1}) {
                int rr = r + d[0] * sgn, cc = c + d[1] * sgn;
                while (inside(rr, cc) && o[rr][cc] == mark) {
                    run++;
                    rr += d[0] * sgn;
                    cc += d[1] * sgn;
                }
                if (inside(rr, cc) && o[rr][cc] == EMPTY) ends++;
            }
            if (run >= 5) return -1;
            total += value(run, ends);
        }
        return total;
    }

    /** Ô có quân nào trong phạm vi 2 ô không (chỉ xét các ô này cho nhanh). */
    boolean nearStone(int r, int c) {
        for (int rr = Math.max(0, r - 2); rr < Math.min(N, r + 3); rr++)
            for (int cc = Math.max(0, c - 2); cc < Math.min(N, c + 3); cc++)
                if (o[rr][cc] != EMPTY) return true;
        return false;
    }

    /** Thắng ngay nếu được, chặn nếu đối thủ sắp thắng, còn lại cộng điểm tấn công + phòng thủ. */
    int[] chooseMove(char me, String style, Lcg rng) {
        char opp = me == 'X' ? 'O' : 'X';
        long[] w = STYLES.get(style);
        int bestR = N / 2, bestC = N / 2, bestDist = 0;   // bàn trống: đánh vào giữa
        long bestScore = -1;
        for (int r = 0; r < N; r++)
            for (int c = 0; c < N; c++) {
                if (o[r][c] != EMPTY || !nearStone(r, c)) continue;
                long mine = lineValue(r, c, me), theirs = lineValue(r, c, opp), score;
                if (mine < 0) score = 1_000_000_000_000L;          // thắng ngay
                else if (theirs < 0) score = 100_000_000_000L;     // bắt buộc chặn
                else {
                    score = w[0] * mine + w[1] * theirs;
                    if (w[2] > 0) score += rng.nextInt(0, (int) w[2]);
                }
                // hòa điểm: gần tâm hơn, rồi theo hàng, cột (duyệt r, c tăng dần nên chỉ cần so "<")
                int dist = Math.abs(r - N / 2) + Math.abs(c - N / 2);
                if (score > bestScore || (score == bestScore && dist < bestDist)) {
                    bestR = r; bestC = c; bestScore = score; bestDist = dist;
                }
            }
        return new int[] {bestR, bestC};
    }
}

namespace BaiMR6_4;

// Bàn cờ Caro 15×15: kiểm tra thắng (5 quân liên tiếp), hiển thị, bot đơn giản.
// Server dùng FiveLine() để PHÁN XỬ (server là bên có thẩm quyền); client dùng Render() để vẽ bàn cờ và
// ChooseMove() khi chạy ở chế độ bot. Bot là hàm tất định nên mọi bản ngôn ngữ cho cùng ván đấu.
static class BanCo
{
    public const int N = 15;                                         // kích thước bàn cờ
    public const char Empty = '.';
    static readonly (int Dr, int Dc)[] Dirs = [(0, 1), (1, 0), (1, 1), (1, -1)];   // ngang, dọc, chéo xuống, chéo lên

    public static char[][] NewBoard() => Enumerable.Range(0, N).Select(_ => Enumerable.Repeat(Empty, N).ToArray()).ToArray();

    public static bool Inside(int r, int c) => 0 <= r && r < N && 0 <= c && c < N;

    /// <summary>(7, 7) -> "h8": cột là chữ a–o, hàng là số 1–15.</summary>
    public static string CellName(int r, int c) => $"{(char)('a' + c)}{r + 1}";

    /// <summary>"h8" -> (7, 7); trả null nếu sai cú pháp hoặc ngoài bàn cờ.</summary>
    public static (int R, int C)? ParseCell(string text)
    {
        text = text.Trim().ToLowerInvariant();
        if (text.Length < 2 || text[0] < 'a' || text[0] > 'o' || !text[1..].All(char.IsAsciiDigit)) return null;
        int r = int.Parse(text[1..]) - 1, c = text[0] - 'a';
        return Inside(r, c) ? (r, c) : null;
    }

    /// <summary>Nếu quân vừa đặt ở (r, c) tạo thành dãy ≥ 5 quân cùng màu thì trả danh sách ô của dãy, ngược lại null.</summary>
    public static List<(int R, int C)>? FiveLine(char[][] board, int r, int c)
    {
        char mark = board[r][c];
        foreach (var (dr, dc) in Dirs)
        {
            var line = new List<(int, int)> { (r, c) };
            foreach (int sgn in new[] { 1, -1 })                     // đi về hai phía của ô vừa đặt
            {
                int rr = r + dr * sgn, cc = c + dc * sgn;
                while (Inside(rr, cc) && board[rr][cc] == mark)
                {
                    line.Add((rr, cc));
                    rr += dr * sgn; cc += dc * sgn;
                }
            }
            if (line.Count >= 5) return line.Order().ToList();
        }
        return null;
    }

    /// <summary>Vẽ bàn cờ thành các dòng chữ; ô vừa đánh (last) được đánh dấu bằng chữ thường x/o.</summary>
    public static List<string> Render(char[][] board, (int R, int C)? last = null)
    {
        var lines = new List<string> { "    " + string.Join(" ", Enumerable.Range(0, N).Select(c => (char)('a' + c))) };
        for (int r = 0; r < N; r++)
        {
            var cells = Enumerable.Range(0, N).Select(c => last == (r, c) ? char.ToLowerInvariant(board[r][c]) : board[r][c]);
            lines.Add($"{r + 1,3} " + string.Join(" ", cells));
        }
        return lines;
    }

    // ---------------------------------------------------------------- bot
    // Giá trị của một hướng theo (độ dài dãy, số đầu còn trống): dãy 4 hở hai đầu gần như chắc thắng.
    static readonly Dictionary<(int, int), long> Value = new()
    {
        [(4, 2)] = 100000, [(4, 1)] = 10000, [(3, 2)] = 5000, [(3, 1)] = 500,
        [(2, 2)] = 200, [(2, 1)] = 50, [(1, 2)] = 10, [(1, 1)] = 2,
    };
    // Kiểu bot: (hệ số tấn công, hệ số phòng thủ, độ "nhiễu"). Bot tập sự cộng thêm điểm ngẫu nhiên nên hay bỏ sót
    // thế cờ nguy hiểm (nhưng vẫn luôn thắng ngay khi có thể và luôn chặn dãy 4).
    public static readonly Dictionary<string, (long Att, long Dfn, long Noise)> Styles = new()
    {
        ["tan_cong"] = (10, 7, 0), ["phong_thu"] = (7, 10, 0), ["can_bang"] = (10, 9, 0), ["tap_su"] = (10, 9, 50000),
    };

    /// <summary>Giá trị của việc đặt quân mark vào ô trống (r, c); trả -1 nếu tạo được 5 quân (thắng ngay).</summary>
    static long LineValue(char[][] board, int r, int c, char mark)
    {
        long total = 0;
        foreach (var (dr, dc) in Dirs)
        {
            int run = 1, ends = 0;
            foreach (int sgn in new[] { 1, -1 })
            {
                int rr = r + dr * sgn, cc = c + dc * sgn;
                while (Inside(rr, cc) && board[rr][cc] == mark)
                {
                    run++;
                    rr += dr * sgn; cc += dc * sgn;
                }
                if (Inside(rr, cc) && board[rr][cc] == Empty) ends++;
            }
            if (run >= 5) return -1;
            total += Value.GetValueOrDefault((run, ends), 0);
        }
        return total;
    }

    /// <summary>Ô có quân nào trong phạm vi 2 ô không (chỉ xét các ô này cho nhanh).</summary>
    static bool NearStone(char[][] board, int r, int c)
    {
        for (int rr = Math.Max(0, r - 2); rr < Math.Min(N, r + 3); rr++)
            for (int cc = Math.Max(0, c - 2); cc < Math.Min(N, c + 3); cc++)
                if (board[rr][cc] != Empty) return true;
        return false;
    }

    /// <summary>Chọn nước đi: thắng ngay nếu được, chặn nếu đối thủ sắp thắng, còn lại cộng điểm tấn công + phòng thủ.</summary>
    public static (int R, int C) ChooseMove(char[][] board, char me, string style, Lcg rng)
    {
        char opp = me == 'X' ? 'O' : 'X';
        var (att, dfn, noise) = Styles[style];
        var best = (N / 2, N / 2);                                   // bàn trống: đánh vào giữa
        (long, int, int, int)? bestKey = null;
        for (int r = 0; r < N; r++)
            for (int c = 0; c < N; c++)
            {
                if (board[r][c] != Empty || !NearStone(board, r, c)) continue;
                long mine = LineValue(board, r, c, me), theirs = LineValue(board, r, c, opp);
                long score;
                if (mine < 0) score = 1_000_000_000_000;                  // thắng ngay
                else if (theirs < 0) score = 100_000_000_000;             // bắt buộc chặn
                else
                {
                    score = att * mine + dfn * theirs;
                    if (noise != 0) score += rng.NextInt(0, noise);
                }
                var key = (-score, Math.Abs(r - N / 2) + Math.Abs(c - N / 2), r, c);   // hòa điểm: gần tâm, rồi theo hàng, cột
                if (bestKey == null || key.CompareTo(bestKey.Value) < 0)
                {
                    best = (r, c);
                    bestKey = key;
                }
            }
        return best;
    }
}

/// <summary>Bộ sinh số giả ngẫu nhiên LCG (cùng công thức với MR3.3) – để ván đấu của bot tái lập được.</summary>
class Lcg(long seed)
{
    long x = seed & 0x7FFFFFFF;

    public long NextInt(long lo, long hi)
    {
        x = (x * 1103515245 + 12345) & 0x7FFFFFFF;
        return lo + (x >> 8) % (hi - lo + 1);
    }
}

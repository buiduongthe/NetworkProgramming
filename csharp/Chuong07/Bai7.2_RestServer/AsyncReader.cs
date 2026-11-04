using System.Text;

namespace Bai7_2;

// Mã giả 3.5 – Reader { sock, pending }, phiên bản bất đồng bộ trên Stream (READ_LINE_ASYNC, READ_EXACT_ASYNC)
sealed class AsyncReader(Stream stream)
{
    byte[] pending = [];

    public async Task<string?> ReadLineAsync(int maxLen)
    {
        while (true)
        {
            int i = Array.IndexOf(pending, (byte)'\n');
            if (i > maxLen) throw new InvalidDataException("Dòng quá dài");   // Mã giả 3.5 dòng 5: có LF nhưng dòng quá dài
            if (i >= 0)
            {
                int len = (i > 0 && pending[i - 1] == '\r') ? i - 1 : i;   // STRIP_CR
                string line = Encoding.UTF8.GetString(pending, 0, len);
                pending = pending[(i + 1)..];
                return line;
            }
            if (pending.Length > maxLen) throw new InvalidDataException("Dòng quá dài");
            var buf = new byte[4096];
            int n = await stream.ReadAsync(buf);
            if (n == 0) return null;                                    // đối tác đã đóng
            pending = [.. pending, .. buf.AsSpan(0, n)];
        }
    }

    // READ_EXACT_ASYNC: lấy phần dư pending trước, thiếu thì đọc tiếp đủ n byte
    public async Task<byte[]> ReadExactAsync(int n)
    {
        var data = new byte[n];
        int have = Math.Min(n, pending.Length);
        Array.Copy(pending, data, have);
        pending = pending[have..];
        await stream.ReadExactlyAsync(data.AsMemory(have));             // ném EndOfStreamException nếu đóng giữa chừng
        return data;
    }

    // Mã giả 7.1 – READ_HEADERS: tên trường đổi về chữ thường
    public async Task<Dictionary<string, string>> ReadHeadersAsync()
    {
        var hdr = new Dictionary<string, string>();
        while (true)
        {
            string? line = await ReadLineAsync(8192);
            if (line == null)                                            // Mã giả 7.1 dòng 29: đóng giữa chừng
                throw new IOException("Kết nối đóng khi chưa đọc hết phần tiêu đề");   // ConnectionClosed
            if (line == "") return hdr;                                  // dòng 30: dòng trống – hết
            int i = line.IndexOf(':');
            string k = i >= 0 ? line[..i] : line, v = i >= 0 ? line[(i + 1)..] : "";
            hdr[k.Trim().ToLowerInvariant()] = v.Trim();
        }
    }
}

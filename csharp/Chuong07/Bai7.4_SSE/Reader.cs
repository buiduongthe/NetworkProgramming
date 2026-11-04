using System.Text;

namespace Bai7_4;

// Mã giả 3.5 – Reader { sock, pending }; nguồn byte là một hàm nhận (socket hoặc lớp giải mã khối)
sealed class Reader(Func<byte[], int, int, int> recv)
{
    byte[] pending = [];

    public string? ReadLine(int maxLen)                                 // READ_LINE: null khi nguồn đã hết
    {
        while (true)
        {
            int i = Array.IndexOf(pending, (byte)'\n');
            if (i > maxLen) throw new InvalidDataException("Dòng quá dài");   // Mã giả 3.5 dòng 5: có LF nhưng dòng quá dài
            if (i >= 0)
            {
                int len = (i > 0 && pending[i - 1] == '\r') ? i - 1 : i;
                string line = Encoding.UTF8.GetString(pending, 0, len);
                pending = pending[(i + 1)..];
                return line;
            }
            if (pending.Length > maxLen) throw new InvalidDataException("Dòng quá dài");
            var buf = new byte[4096];
            int n = recv(buf, 0, buf.Length);
            if (n == 0) return null;
            pending = [.. pending, .. buf.AsSpan(0, n)];
        }
    }

    public byte[] ReadExact(int n)                                      // READ_EXACT
    {
        var data = new byte[n];
        int have = Math.Min(n, pending.Length);
        Array.Copy(pending, data, have);
        pending = pending[have..];
        while (have < n)
        {
            int k = recv(data, have, n - have);
            if (k == 0) throw new IOException("Kết nối đóng giữa chừng");
            have += k;
        }
        return data;
    }

    public Dictionary<string, string> ReadHeaders()                    // READ_HEADERS (Mã giả 7.1)
    {
        var hdr = new Dictionary<string, string>();
        while (true)
        {
            string? line = ReadLine(8192);
            if (line == null)                                            // Mã giả 7.1 dòng 29: đóng giữa chừng
                throw new IOException("Kết nối đóng khi chưa đọc hết phần tiêu đề");   // ConnectionClosed
            if (line == "") return hdr;                                  // dòng 30: dòng trống – hết
            int i = line.IndexOf(':');
            string k = i >= 0 ? line[..i] : line, v = i >= 0 ? line[(i + 1)..] : "";
            hdr[k.Trim().ToLowerInvariant()] = v.Trim();
        }
    }
}

// Lớp giải mã khối (READ_CHUNKED của Mã giả 7.1 viết dạng luồng): Recv() trả dữ liệu thân thuần
sealed class ChunkedSource(Reader r)
{
    int left; bool done;

    public int Recv(byte[] buf, int off, int count)
    {
        if (done) return 0;
        if (left == 0)
        {
            int size = Convert.ToInt32((r.ReadLine(1024) ?? "0").Split(';')[0].Trim(), 16);
            if (size == 0) { r.ReadHeaders(); done = true; return 0; }  // khối 0: hết thân, bỏ trailer
            left = size;
        }
        byte[] data = r.ReadExact(Math.Min(count, left));
        data.CopyTo(buf, off);
        left -= data.Length;
        if (left == 0) r.ReadLine(2);                                   // CRLF sau mỗi khối
        return data.Length;
    }
}

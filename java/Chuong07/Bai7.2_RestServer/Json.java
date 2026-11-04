import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * JSON tối giản cho bài 7.2 (JDK không có thư viện JSON chuẩn):
 * encode() cho Map, List, String, Number; parseObject() cho đối tượng phẳng {"khóa":"chuỗi",...}.
 */
final class Json {
    private Json() {}

    static String encode(Object o) {
        if (o == null) return "null";
        if (o instanceof String s) return quote(s);
        if (o instanceof Number || o instanceof Boolean) return o.toString();
        StringBuilder b = new StringBuilder();
        if (o instanceof Map<?, ?> m) {
            b.append('{');
            for (var e : m.entrySet()) {
                if (b.length() > 1) b.append(',');
                b.append(quote(e.getKey().toString())).append(':').append(encode(e.getValue()));
            }
            return b.append('}').toString();
        }
        if (o instanceof List<?> l) {
            b.append('[');
            for (Object x : l) { if (b.length() > 1) b.append(','); b.append(encode(x)); }
            return b.append(']').toString();
        }
        throw new IllegalArgumentException("Kiểu không hỗ trợ: " + o.getClass());
    }

    static String quote(String s) {
        StringBuilder b = new StringBuilder("\"");
        for (char c : s.toCharArray()) {
            switch (c) {
                case '"' -> b.append("\\\"");
                case '\\' -> b.append("\\\\");
                case '\n' -> b.append("\\n");
                case '\r' -> b.append("\\r");
                case '\t' -> b.append("\\t");
                default -> { if (c < 0x20) b.append(String.format("\\u%04x", (int) c)); else b.append(c); }
            }
        }
        return b.append('"').toString();
    }

    /** Phân tích đối tượng phẳng chỉ gồm giá trị chuỗi; ném IllegalArgumentException nếu sai cú pháp. */
    static Map<String, String> parseObject(String s) {
        Map<String, String> m = new LinkedHashMap<>();
        int[] i = {skip(s, 0)};
        expect(s, i, '{');
        if (peek(s, i) == '}') { i[0]++; return m; }
        while (true) {
            String k = parseString(s, i);
            expect(s, i, ':');
            m.put(k, parseString(s, i));
            char c = peek(s, i); i[0]++;
            if (c == '}') break;
            if (c != ',') throw new IllegalArgumentException("JSON sai");
        }
        if (skip(s, i[0]) != s.length()) throw new IllegalArgumentException("JSON sai");
        return m;
    }

    private static String parseString(String s, int[] i) {
        expect(s, i, '"');
        StringBuilder b = new StringBuilder();
        while (i[0] < s.length()) {
            char c = s.charAt(i[0]++);
            if (c == '"') return b.toString();
            if (c != '\\') { b.append(c); continue; }
            char e = s.charAt(i[0]++);
            switch (e) {
                case 'n' -> b.append('\n');
                case 't' -> b.append('\t');
                case 'r' -> b.append('\r');
                case 'u' -> { b.append((char) Integer.parseInt(s.substring(i[0], i[0] + 4), 16)); i[0] += 4; }
                default -> b.append(e);
            }
        }
        throw new IllegalArgumentException("JSON sai: chuỗi chưa đóng");
    }

    private static char peek(String s, int[] i) {
        i[0] = skip(s, i[0]);
        if (i[0] >= s.length()) throw new IllegalArgumentException("JSON sai: thiếu dữ liệu");
        return s.charAt(i[0]);
    }

    private static void expect(String s, int[] i, char c) {
        if (peek(s, i) != c) throw new IllegalArgumentException("JSON sai: cần '" + c + "'");
        i[0]++;
    }

    private static int skip(String s, int i) {
        while (i < s.length() && Character.isWhitespace(s.charAt(i))) i++;
        return i;
    }
}

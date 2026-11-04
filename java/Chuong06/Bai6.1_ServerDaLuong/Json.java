import java.util.ArrayList;
import java.util.Collection;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * JSON tối giản cho các bài Chương 6 (JDK không có thư viện JSON chuẩn).
 * JSON_ENCODE: Map, List, String, số, boolean, null → văn bản; JSON_DECODE: văn bản → Map/List/String/Long/Double.
 */
final class Json {
    private Json() {}

    /** JsonError của mã giả: văn bản không phải JSON hợp lệ. */
    static final class JsonError extends RuntimeException { JsonError(String m) { super(m); } }

    /** Tạo đối tượng JSON giữ thứ tự trường: obj("status", 200, "error", "..."). */
    static Map<String, Object> obj(Object... kv) {
        Map<String, Object> m = new LinkedHashMap<>();
        for (int i = 0; i + 1 < kv.length; i += 2) m.put((String) kv[i], kv[i + 1]);
        return m;
    }

    static String encode(Object v) { StringBuilder sb = new StringBuilder(); write(sb, v); return sb.toString(); }

    private static void write(StringBuilder sb, Object v) {
        if (v == null) sb.append("null");
        else if (v instanceof Number || v instanceof Boolean) sb.append(v);
        else if (v instanceof Map<?, ?> m) {
            sb.append('{');
            for (Map.Entry<?, ?> e : m.entrySet()) {
                if (sb.charAt(sb.length() - 1) != '{') sb.append(',');
                str(sb, String.valueOf(e.getKey()));
                sb.append(':');
                write(sb, e.getValue());
            }
            sb.append('}');
        } else if (v instanceof Collection<?> c) {
            sb.append('[');
            for (Object o : c) {
                if (sb.charAt(sb.length() - 1) != '[') sb.append(',');
                write(sb, o);
            }
            sb.append(']');
        } else str(sb, v.toString());
    }

    private static void str(StringBuilder sb, String s) {
        sb.append('"');
        for (char ch : s.toCharArray()) {
            switch (ch) {
                case '"' -> sb.append("\\\"");
                case '\\' -> sb.append("\\\\");
                case '\n' -> sb.append("\\n");
                case '\r' -> sb.append("\\r");
                case '\t' -> sb.append("\\t");
                default -> sb.append(ch < 0x20 ? String.format("\\u%04x", (int) ch) : String.valueOf(ch));
            }
        }
        sb.append('"');
    }

    /** Giải mã và yêu cầu kết quả là đối tượng JSON. */
    @SuppressWarnings("unchecked")
    static Map<String, Object> decodeObject(String text) {
        Parser p = new Parser(text);
        Object v = p.value();
        p.ws();
        if (p.i != text.length()) throw new JsonError("Dư ký tự sau JSON");
        if (!(v instanceof Map)) throw new JsonError("Thông điệp không phải đối tượng JSON");
        return (Map<String, Object>) v;
    }

    private static final class Parser {
        final String s;
        int i;

        Parser(String s) { this.s = s; }

        void ws() { while (i < s.length() && Character.isWhitespace(s.charAt(i))) i++; }

        char next() { if (i >= s.length()) throw new JsonError("JSON bị cắt cụt"); return s.charAt(i++); }

        Object value() {
            ws();
            if (i >= s.length()) throw new JsonError("JSON rỗng");
            char c = s.charAt(i);
            if (c == '{') {
                i++;
                Map<String, Object> m = new LinkedHashMap<>();
                ws();
                if (s.startsWith("}", i)) { i++; return m; }
                while (true) {
                    ws();
                    if (next() != '"') throw new JsonError("Thiếu tên trường");
                    String k = string();
                    ws();
                    if (next() != ':') throw new JsonError("Thiếu dấu ':'");
                    m.put(k, value());
                    ws();
                    char d = next();
                    if (d == '}') return m;
                    if (d != ',') throw new JsonError("Thiếu dấu ','");
                }
            }
            if (c == '[') {
                i++;
                List<Object> a = new ArrayList<>();
                ws();
                if (s.startsWith("]", i)) { i++; return a; }
                while (true) {
                    a.add(value());
                    ws();
                    char d = next();
                    if (d == ']') return a;
                    if (d != ',') throw new JsonError("Thiếu dấu ','");
                }
            }
            if (c == '"') { i++; return string(); }
            if (s.startsWith("true", i)) { i += 4; return Boolean.TRUE; }
            if (s.startsWith("false", i)) { i += 5; return Boolean.FALSE; }
            if (s.startsWith("null", i)) { i += 4; return null; }
            int st = i;
            while (i < s.length() && "+-0123456789.eE".indexOf(s.charAt(i)) >= 0) i++;
            String num = s.substring(st, i);
            try {
                return num.matches("-?\\d+") ? (Object) Long.parseLong(num) : (Object) Double.parseDouble(num);
            } catch (NumberFormatException e) {
                throw new JsonError("Giá trị không hợp lệ tại vị trí " + st);
            }
        }

        String string() {
            StringBuilder sb = new StringBuilder();
            while (true) {
                char c = next();
                if (c == '"') return sb.toString();
                if (c != '\\') { sb.append(c); continue; }
                char e = next();
                switch (e) {
                    case 'n' -> sb.append('\n');
                    case 't' -> sb.append('\t');
                    case 'r' -> sb.append('\r');
                    case 'b', 'f' -> sb.append(e == 'b' ? '\b' : '\f');
                    case 'u' -> {
                        if (i + 4 > s.length()) throw new JsonError("Mã \\u không đủ 4 chữ số");
                        sb.append((char) Integer.parseInt(s.substring(i, i + 4), 16));
                        i += 4;
                    }
                    default -> sb.append(e);
                }
            }
        }
    }
}

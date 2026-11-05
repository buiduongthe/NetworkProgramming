import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * JSON_ENCODE / JSON_DECODE tối giản (JDK không có thư viện JSON chuẩn).
 * Ánh xạ: object -> Map<String,Object>, array -> List<Object>, chuỗi -> String,
 * số nguyên -> Long, số thực -> Double, true/false -> Boolean, null -> null.
 */
public final class Json {
    private final String s;
    private int i;

    private Json(String s) { this.s = s; }

    /** JSON_DECODE: ném IllegalArgumentException khi văn bản không phải JSON hợp lệ. */
    public static Object parse(String text) {
        Json p = new Json(text);
        p.ws();
        Object v = p.value();
        p.ws();
        if (p.i != p.s.length()) throw p.err("thừa ký tự");
        return v;
    }

    /** JSON_ENCODE: một dòng, ký tự điều khiển (kể cả xuống dòng) được thoát. */
    public static String stringify(Object v) {
        StringBuilder b = new StringBuilder();
        write(b, v);
        return b.toString();
    }

    private IllegalArgumentException err(String m) { return new IllegalArgumentException("JSON lỗi tại " + i + ": " + m); }

    private void ws() { while (i < s.length() && " \t\r\n".indexOf(s.charAt(i)) >= 0) i++; }

    private Object value() {
        if (i >= s.length()) throw err("hết dữ liệu");
        char c = s.charAt(i);
        if (c == '{') return object();
        if (c == '[') return array();
        if (c == '"') return string();
        if (s.startsWith("true", i)) { i += 4; return Boolean.TRUE; }
        if (s.startsWith("false", i)) { i += 5; return Boolean.FALSE; }
        if (s.startsWith("null", i)) { i += 4; return null; }
        return number();
    }

    private Map<String, Object> object() {
        Map<String, Object> m = new LinkedHashMap<>();
        i++; ws();
        if (i < s.length() && s.charAt(i) == '}') { i++; return m; }
        while (true) {
            ws();
            if (i >= s.length() || s.charAt(i) != '"') throw err("cần tên trường");
            String k = string();
            ws();
            if (i >= s.length() || s.charAt(i++) != ':') throw err("cần ':'");
            ws();
            m.put(k, value());
            ws();
            if (i >= s.length()) throw err("hết dữ liệu");
            char c = s.charAt(i++);
            if (c == '}') return m;
            if (c != ',') throw err("cần ',' hoặc '}'");
        }
    }

    private List<Object> array() {
        List<Object> a = new ArrayList<>();
        i++; ws();
        if (i < s.length() && s.charAt(i) == ']') { i++; return a; }
        while (true) {
            ws();
            a.add(value());
            ws();
            if (i >= s.length()) throw err("hết dữ liệu");
            char c = s.charAt(i++);
            if (c == ']') return a;
            if (c != ',') throw err("cần ',' hoặc ']'");
        }
    }

    private String string() {
        StringBuilder b = new StringBuilder();
        i++;                                                    // bỏ dấu "
        while (true) {
            if (i >= s.length()) throw err("chuỗi chưa đóng");
            char c = s.charAt(i++);
            if (c == '"') return b.toString();
            if (c != '\\') { b.append(c); continue; }
            if (i >= s.length()) throw err("thoát dở");
            char e = s.charAt(i++);
            switch (e) {
                case 'n' -> b.append('\n');  case 't' -> b.append('\t');
                case 'r' -> b.append('\r');  case 'b' -> b.append('\b');
                case 'f' -> b.append('\f');
                case 'u' -> {
                    if (i + 4 > s.length()) throw err("\\u thiếu chữ số");
                    b.append((char) Integer.parseInt(s.substring(i, i + 4), 16)); i += 4;
                }
                default -> b.append(e);                          // \" \\ \/
            }
        }
    }

    private Object number() {
        int st = i;
        while (i < s.length() && "+-0123456789.eE".indexOf(s.charAt(i)) >= 0) i++;
        String t = s.substring(st, i);
        if (t.isEmpty()) throw err("giá trị không hợp lệ");
        try {
            if (t.contains(".") || t.contains("e") || t.contains("E")) return Double.parseDouble(t);
            return Long.parseLong(t);
        } catch (NumberFormatException ex) { throw err("số không hợp lệ"); }
    }

    @SuppressWarnings("unchecked")
    private static void write(StringBuilder b, Object v) {
        if (v == null) b.append("null");
        else if (v instanceof String str) quote(b, str);
        else if (v instanceof Number || v instanceof Boolean) b.append(v);
        else if (v instanceof Map<?, ?> m) {
            b.append('{');
            boolean first = true;
            for (Map.Entry<?, ?> e : m.entrySet()) {
                if (!first) b.append(',');
                first = false;
                quote(b, String.valueOf(e.getKey())); b.append(':'); write(b, e.getValue());
            }
            b.append('}');
        } else if (v instanceof List<?> a) {
            b.append('[');
            for (int k = 0; k < a.size(); k++) { if (k > 0) b.append(','); write(b, a.get(k)); }
            b.append(']');
        } else quote(b, v.toString());
    }

    private static void quote(StringBuilder b, String str) {
        b.append('"');
        for (char c : str.toCharArray()) {
            switch (c) {
                case '"' -> b.append("\\\"");  case '\\' -> b.append("\\\\");
                case '\n' -> b.append("\\n");  case '\r' -> b.append("\\r");  case '\t' -> b.append("\\t");
                default -> { if (c < 0x20) b.append(String.format("\\u%04x", (int) c)); else b.append(c); }
            }
        }
        b.append('"');
    }

    /** Tiện ích: tạo Map có thứ tự từ các cặp khóa, giá trị. */
    public static Map<String, Object> obj(Object... kv) {
        Map<String, Object> m = new LinkedHashMap<>();
        for (int k = 0; k + 1 < kv.length; k += 2) m.put((String) kv[k], kv[k + 1]);
        return m;
    }
}

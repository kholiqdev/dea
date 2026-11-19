// Finds colours written straight into source. Components take colours from
// tokens.css only (F1-FON-16).

const HEX_LITERAL =
  /#(?:[0-9a-f]{8}|[0-9a-f]{6}|[0-9a-f]{3,4})(?![0-9a-z_-])/gi;
const COLOR_FUNCTION = /\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch)\(/gi;
// A declaration whose property takes a colour, and its value.
const COLOR_DECLARATION =
  /(?:^|[\s;{"'])((?:[a-z]+-)*color|background|border(?:-[a-z]+)*|outline|fill|stroke|box-shadow|text-shadow|caret-color|accent-color|text-decoration)\s*:\s*([^;{}"'\n]*)/gi;

// CSS named colours, without the allowed keywords (transparent,
// currentColor, inherit).
const NAMED = new Set(
  (
    "aliceblue antiquewhite aqua aquamarine azure beige bisque black " +
    "blanchedalmond blue blueviolet brown burlywood cadetblue chartreuse " +
    "chocolate coral cornflowerblue cornsilk crimson cyan darkblue darkcyan " +
    "darkgoldenrod darkgray darkgreen darkgrey darkkhaki darkmagenta " +
    "darkolivegreen darkorange darkorchid darkred darksalmon darkseagreen " +
    "darkslateblue darkslategray darkslategrey darkturquoise darkviolet " +
    "deeppink deepskyblue dimgray dimgrey dodgerblue firebrick floralwhite " +
    "forestgreen fuchsia gainsboro ghostwhite gold goldenrod gray green " +
    "greenyellow grey honeydew hotpink indianred indigo ivory khaki lavender " +
    "lavenderblush lawngreen lemonchiffon lightblue lightcoral lightcyan " +
    "lightgoldenrodyellow lightgray lightgreen lightgrey lightpink " +
    "lightsalmon lightseagreen lightskyblue lightslategray lightslategrey " +
    "lightsteelblue lightyellow lime limegreen linen magenta maroon " +
    "mediumaquamarine mediumblue mediumorchid mediumpurple mediumseagreen " +
    "mediumslateblue mediumspringgreen mediumturquoise mediumvioletred " +
    "midnightblue mintcream mistyrose moccasin navajowhite navy oldlace olive " +
    "olivedrab orange orangered orchid palegoldenrod palegreen " +
    "paleturquoise palevioletred papayawhip peachpuff peru pink plum " +
    "powderblue purple rebeccapurple red rosybrown royalblue saddlebrown " +
    "salmon sandybrown seagreen seashell sienna silver skyblue slateblue " +
    "slategray slategrey snow springgreen steelblue tan teal thistle tomato " +
    "turquoise violet wheat white whitesmoke yellow yellowgreen"
  ).split(" "),
);

/**
 * Every colour literal in `source`, in order of appearance: hex colours,
 * the name and opening bracket of colour functions, and named colours used
 * as a colour value.
 */
export function findColorLiterals(source: string): string[] {
  const found: { at: number; text: string }[] = [];
  for (const m of source.matchAll(HEX_LITERAL))
    found.push({ at: m.index, text: m[0] });
  for (const m of source.matchAll(COLOR_FUNCTION))
    found.push({ at: m.index, text: m[0] });
  for (const m of source.matchAll(COLOR_DECLARATION)) {
    const value = m[2] ?? "";
    const start = m.index + m[0].length - value.length;
    for (const word of value.matchAll(/[a-z]+/gi))
      if (NAMED.has(word[0].toLowerCase()) && !/var\(/.test(value))
        found.push({ at: start + word.index, text: word[0] });
  }
  return found.sort((a, b) => a.at - b.at).map((f) => f.text);
}

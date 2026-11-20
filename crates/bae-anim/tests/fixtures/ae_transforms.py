#!/usr/bin/env python3
"""Transform reference data for tests/ae_transforms.rs.

  python3 ae_transforms.py jsx OUT.jsx            # write the AE sampling script
  tools/ae/aectl run OUT.jsx                       # writes /private/tmp/bae-03/xf/ae-transforms.json
  python3 ae_transforms.py merge RESULT.json OUT.json

Cross-layer expressions block when read from a script in AE 26, so the script adds
3D point controls with `toWorld(p)` / `toComp(p)` to each layer itself.
"""
import json
import sys

CASES = [
 {
  "id": "2d_basic",
  "layers": [
   {
    "name": "A",
    "anchor": [
     50,
     50,
     0
    ],
    "position": [
     960,
     540,
     0
    ],
    "scale": [
     200,
     50,
     100
    ],
    "rotation": 30
   }
  ],
  "times": [
   0
  ],
  "points": [
   [
    0,
    0,
    0
   ],
   [
    100,
    0,
    0
   ],
   [
    0,
    100,
    0
   ],
   [
    0,
    0,
    100
   ],
   [
    37,
    -21,
    13
   ]
  ]
 },
 {
  "id": "2d_negative_scale",
  "layers": [
   {
    "name": "A",
    "anchor": [
     10,
     -20,
     0
    ],
    "position": [
     300,
     200,
     0
    ],
    "scale": [
     -150,
     80,
     100
    ],
    "rotation": -75
   }
  ],
  "times": [
   0
  ],
  "points": [
   [
    0,
    0,
    0
   ],
   [
    100,
    0,
    0
   ],
   [
    0,
    100,
    0
   ],
   [
    0,
    0,
    100
   ],
   [
    37,
    -21,
    13
   ]
  ]
 },
 {
  "id": "3d_order",
  "layers": [
   {
    "name": "A",
    "three_d": True,
    "anchor": [
     10,
     20,
     30
    ],
    "position": [
     900,
     500,
     100
    ],
    "scale": [
     80,
     120,
     150
    ],
    "orientation": [
     10,
     20,
     30
    ],
    "rotation_x": 40,
    "rotation_y": 50,
    "rotation": 60
   }
  ],
  "times": [
   0
  ],
  "points": [
   [
    0,
    0,
    0
   ],
   [
    100,
    0,
    0
   ],
   [
    0,
    100,
    0
   ],
   [
    0,
    0,
    100
   ],
   [
    37,
    -21,
    13
   ]
  ]
 },
 {
  "id": "3d_orientation_only",
  "layers": [
   {
    "name": "A",
    "three_d": True,
    "position": [
     0,
     0,
     0
    ],
    "orientation": [
     35,
     -60,
     120
    ]
   }
  ],
  "times": [
   0
  ],
  "points": [
   [
    0,
    0,
    0
   ],
   [
    100,
    0,
    0
   ],
   [
    0,
    100,
    0
   ],
   [
    0,
    0,
    100
   ],
   [
    37,
    -21,
    13
   ]
  ]
 },
 {
  "id": "parent_2d_skew",
  "layers": [
   {
    "name": "P",
    "anchor": [
     20,
     10,
     0
    ],
    "position": [
     500,
     400,
     0
    ],
    "scale": [
     200,
     50,
     100
    ],
    "rotation": 20
   },
   {
    "name": "C",
    "parent": "P",
    "anchor": [
     5,
     5,
     0
    ],
    "position": [
     60,
     -30,
     0
    ],
    "scale": [
     90,
     110,
     100
    ],
    "rotation": 45
   }
  ],
  "times": [
   0
  ],
  "points": [
   [
    0,
    0,
    0
   ],
   [
    100,
    0,
    0
   ],
   [
    0,
    100,
    0
   ],
   [
    0,
    0,
    100
   ],
   [
    37,
    -21,
    13
   ]
  ]
 },
 {
  "id": "parent_3d_chain",
  "layers": [
   {
    "name": "P",
    "three_d": True,
    "position": [
     600,
     300,
     200
    ],
    "orientation": [
     0,
     30,
     0
    ],
    "rotation_x": 15,
    "scale": [
     120,
     120,
     120
    ]
   },
   {
    "name": "C",
    "three_d": True,
    "parent": "P",
    "position": [
     50,
     60,
     70
    ],
    "rotation_y": -40,
    "rotation": 10,
    "anchor": [
     5,
     6,
     7
    ]
   }
  ],
  "times": [
   0
  ],
  "points": [
   [
    0,
    0,
    0
   ],
   [
    100,
    0,
    0
   ],
   [
    0,
    100,
    0
   ],
   [
    0,
    0,
    100
   ],
   [
    37,
    -21,
    13
   ]
  ]
 },
 {
  "id": "mixed_2d_child_of_3d",
  "layers": [
   {
    "name": "P",
    "three_d": True,
    "position": [
     700,
     400,
     500
    ],
    "rotation_y": 60,
    "rotation_x": 20
   },
   {
    "name": "C",
    "parent": "P",
    "position": [
     100,
     50,
     0
    ],
    "rotation": 30,
    "scale": [
     150,
     100,
     100
    ]
   }
  ],
  "times": [
   0
  ],
  "points": [
   [
    0,
    0,
    0
   ],
   [
    100,
    0,
    0
   ],
   [
    0,
    100,
    0
   ],
   [
    0,
    0,
    100
   ],
   [
    37,
    -21,
    13
   ]
  ]
 },
 {
  "id": "mixed_3d_child_of_2d",
  "layers": [
   {
    "name": "P",
    "position": [
     400,
     300,
     0
    ],
    "rotation": 25,
    "scale": [
     200,
     100,
     100
    ]
   },
   {
    "name": "C",
    "three_d": True,
    "parent": "P",
    "position": [
     30,
     40,
     50
    ],
    "rotation_y": 45
   }
  ],
  "times": [
   0
  ],
  "points": [
   [
    0,
    0,
    0
   ],
   [
    100,
    0,
    0
   ],
   [
    0,
    100,
    0
   ],
   [
    0,
    0,
    100
   ],
   [
    37,
    -21,
    13
   ]
  ]
 },
 {
  "id": "auto_orient_2d",
  "layers": [
   {
    "name": "A",
    "auto_orient": "along_path",
    "rotation": 10,
    "anchor": [
     5,
     5,
     0
    ],
    "position_keys": [
     [
      0,
      [
       100,
       500,
       0
      ],
      [
       0,
       0,
       0
      ],
      [
       200,
       -300,
       0
      ]
     ],
     [
      2,
      [
       900,
       500,
       0
      ],
      [
       -150,
       -250,
       0
      ],
      [
       0,
       0,
       0
      ]
     ]
    ]
   }
  ],
  "times": [
   0,
   0.25,
   0.5,
   1.0,
   1.5,
   1.99,
   2.5
  ],
  "points": [
   [
    0,
    0,
    0
   ],
   [
    100,
    0,
    0
   ],
   [
    0,
    100,
    0
   ],
   [
    0,
    0,
    100
   ],
   [
    37,
    -21,
    13
   ]
  ]
 },
 {
  "id": "auto_orient_3d",
  "layers": [
   {
    "name": "A",
    "three_d": True,
    "auto_orient": "along_path",
    "rotation_x": 10,
    "position_keys": [
     [
      0,
      [
       100,
       500,
       0
      ],
      [
       0,
       0,
       0
      ],
      [
       200,
       -300,
       300
      ]
     ],
     [
      2,
      [
       900,
       500,
       400
      ],
      [
       -150,
       -250,
       0
      ],
      [
       0,
       0,
       0
      ]
     ]
    ]
   }
  ],
  "times": [
   0.25,
   0.5,
   1.0,
   1.5
  ],
  "points": [
   [
    0,
    0,
    0
   ],
   [
    100,
    0,
    0
   ],
   [
    0,
    100,
    0
   ],
   [
    0,
    0,
    100
   ],
   [
    37,
    -21,
    13
   ]
  ]
 },
 {
  "id": "mixed_2d_child_of_3d_full",
  "layers": [
   {
    "name": "P",
    "three_d": True,
    "anchor": [
     10,
     20,
     30
    ],
    "position": [
     700,
     400,
     500
    ],
    "scale": [
     150,
     80,
     120
    ],
    "rotation": 25,
    "rotation_x": 20,
    "rotation_y": 60,
    "orientation": [
     0,
     0,
     15
    ]
   },
   {
    "name": "C",
    "parent": "P",
    "position": [
     100,
     50,
     0
    ],
    "rotation": 30,
    "scale": [
     150,
     100,
     100
    ],
    "anchor": [
     5,
     5,
     0
    ]
   }
  ],
  "times": [
   0
  ],
  "points": [
   [
    0,
    0,
    0
   ],
   [
    100,
    0,
    0
   ],
   [
    0,
    100,
    0
   ],
   [
    0,
    0,
    100
   ],
   [
    37,
    -21,
    13
   ]
  ]
 },
 {
  "id": "mixed_2d_child_of_3d_orient",
  "layers": [
   {
    "name": "P",
    "three_d": True,
    "position": [
     600,
     300,
     -200
    ],
    "orientation": [
     30,
     0,
     40
    ]
   },
   {
    "name": "C",
    "parent": "P",
    "position": [
     100,
     0,
     0
    ]
   }
  ],
  "times": [
   0
  ],
  "points": [
   [
    0,
    0,
    0
   ],
   [
    100,
    0,
    0
   ],
   [
    0,
    100,
    0
   ],
   [
    0,
    0,
    100
   ],
   [
    37,
    -21,
    13
   ]
  ]
 },
 {
  "id": "mixed_2d_grandchild",
  "layers": [
   {
    "name": "G",
    "position": [
     200,
     100,
     0
    ],
    "rotation": 10,
    "scale": [
     110,
     90,
     100
    ]
   },
   {
    "name": "P",
    "three_d": True,
    "parent": "G",
    "position": [
     300,
     200,
     100
    ],
    "rotation_y": 45,
    "rotation": 20,
    "scale": [
     120,
     120,
     120
    ]
   },
   {
    "name": "C",
    "parent": "P",
    "position": [
     50,
     50,
     0
    ],
    "rotation": -15
   }
  ],
  "times": [
   0
  ],
  "points": [
   [
    0,
    0,
    0
   ],
   [
    100,
    0,
    0
   ],
   [
    0,
    100,
    0
   ],
   [
    0,
    0,
    100
   ],
   [
    37,
    -21,
    13
   ]
  ]
 }
]

JSX = r'''
(function () {
    function n(x) { return x.toPrecision(15); }
    var out = [];
    for (var ci = 0; ci < CASES.length; ci++) {
        var cs = CASES[ci];
        var comp = app.project.items.addComp("bae03_xf_" + cs.id, 1920, 1080, 1, 4, 30);
        var made = {};
        // Create bottom-up so that the first listed layer ends up on top.
        for (var li = cs.layers.length - 1; li >= 0; li--) {
            var d = cs.layers[li];
            var L = comp.layers.addNull(4);
            L.name = d.name;
            if (d.three_d) L.threeDLayer = true;
            var tg = L.property("ADBE Transform Group");
            var an = d.anchor || [0, 0, 0], po = d.position || [960, 540, 0];
            tg.property("ADBE Anchor Point").setValue(d.three_d ? an : [an[0], an[1]]);
            tg.property("ADBE Position").setValue(d.three_d ? po : [po[0], po[1]]);
            if (d.scale) tg.property("ADBE Scale").setValue(d.three_d ? d.scale : [d.scale[0], d.scale[1]]);
            if (d.rotation !== undefined) tg.property("ADBE Rotate Z").setValue(d.rotation);
            if (d.three_d) {
                if (d.orientation) tg.property("ADBE Orientation").setValue(d.orientation);
                if (d.rotation_x !== undefined) tg.property("ADBE Rotate X").setValue(d.rotation_x);
                if (d.rotation_y !== undefined) tg.property("ADBE Rotate Y").setValue(d.rotation_y);
            }
            if (d.position_keys) {
                var pos = tg.property("ADBE Position");
                for (var k = 0; k < d.position_keys.length; k++) {
                    var pk = d.position_keys[k];
                    pos.setValueAtTime(pk[0], d.three_d ? pk[1] : [pk[1][0], pk[1][1]]);
                }
                for (var k = 0; k < d.position_keys.length; k++) {
                    var pk = d.position_keys[k];
                    pos.setSpatialAutoBezierAtKey(k + 1, false);
                    pos.setSpatialContinuousAtKey(k + 1, false);
                    pos.setInterpolationTypeAtKey(k + 1, KeyframeInterpolationType.LINEAR, KeyframeInterpolationType.LINEAR);
                    pos.setSpatialTangentsAtKey(k + 1, pk[2], pk[3]);
                }
            }
            if (d.auto_orient === "along_path") L.autoOrient = AutoOrientType.ALONG_PATH;
            made[d.name] = L;
        }
        for (var li = 0; li < cs.layers.length; li++) {
            var d = cs.layers[li];
            if (d.parent) made[d.name].setParentWithJump(made[d.parent]);
        }
        // Probe: 3D point controls on each layer itself with `toComp(point)` (cross-layer
        // expressions block when read from a script in AE 26).
        var res = [];
        for (var li = 0; li < cs.layers.length; li++) {
            var d = cs.layers[li];
            for (var pi = 0; pi < cs.points.length; pi++) {
                var p = cs.points[pi];
                // Adding an effect invalidates references to earlier ones: re-fetch by index.
                var parade = made[d.name].property("ADBE Effect Parade");
                parade.addProperty("ADBE Point3D Control");
                parade.addProperty("ADBE Point3D Control");
                parade = made[d.name].property("ADBE Effect Parade");
                var fw = parade.property(parade.numProperties - 1);
                fw.property(1).expression = 'toWorld([' + p.join(",") + '])';
                parade = made[d.name].property("ADBE Effect Parade");
                var fc = parade.property(parade.numProperties);
                fc.property(1).expression = 'toComp([' + p.join(",") + '])';
                parade = made[d.name].property("ADBE Effect Parade");
                fw = parade.property(parade.numProperties - 1);
                fc = parade.property(parade.numProperties);
                for (var ti = 0; ti < cs.times.length; ti++) {
                    var w = fw.property(1).valueAtTime(cs.times[ti], false);
                    var v = fc.property(1).valueAtTime(cs.times[ti], false);
                    res.push('{"layer":"' + d.name + '","point":' + pi + ',"time":' + cs.times[ti] + ',"world":[' + n(w[0]) + "," + n(w[1]) + "," + n(w[2]) +
                        '],"comp":[' + n(v[0]) + "," + n(v[1]) + "," + n(v[2]) + "]}");
                }
            }
        }
        out.push('{"id":"' + cs.id + '","samples":[' + res.join(",") + "]}");
        comp.remove();
    }
    var f = new File("/private/tmp/bae-03/xf/ae-transforms.json");
    f.encoding = "UTF-8";
    f.open("w"); f.write('{"host":"' + app.version + '","cases":[' + out.join(",") + "]}"); f.close();
    return "ok " + out.length;
})();
'''


def main():
    if sys.argv[1] == "jsx":
        data = json.dumps([dict(c, times=c.get("times", [0])) for c in CASES])
        open(sys.argv[2], "w").write("var CASES = " + data + ";\n" + JSX)
    elif sys.argv[1] == "merge":
        ae = json.load(open(sys.argv[2]))
        res = {c["id"]: c["samples"] for c in ae["cases"]}
        out = []
        for c in CASES:
            c = dict(c, times=c.get("times", [0]))
            c["samples"] = res[c["id"]]
            out.append(c)
        json.dump({"format": "bae-anim-ae-transforms", "format_version": 1,
                   "host": "After Effects " + ae["host"],
                   "note": "toWorld / toComp of points of each layer (ae_transforms.py)",
                   "cases": out}, open(sys.argv[3], "w"), separators=(",", ":"))


if __name__ == "__main__":
    main()

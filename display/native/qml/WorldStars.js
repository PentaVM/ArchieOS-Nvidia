.pragma library
.import "../../shell/prototype/Lang.js" as Tr
// The sky behind World: the naked-eye stars that draw the best-known constellations, at
// their J2000 positions (right ascension in hours, declination in degrees, rounded to about
// an arc-minute) and their visual magnitude. Positions are the stars' catalogue values,
// not an artist's; the figures are the usual stick figures. Precession since J2000 (about
// 0.4 degrees) is below what a background can show.
//
// Each star: [id, right ascension (h), declination (deg), magnitude].
var stars = [
    // Orion
    ["betelgeuse", 5.9195, 7.4071, 0.50], ["rigel", 5.2423, -8.2017, 0.13], ["bellatrix", 5.4189, 6.3497, 1.64],
    ["mintaka", 5.5334, -0.2991, 2.23], ["alnilam", 5.6036, -1.2019, 1.69], ["alnitak", 5.6793, -1.9428, 1.77],
    ["saiph", 5.7959, -9.6697, 2.09], ["meissa", 5.5856, 9.9342, 3.39],
    // Canis Major, Canis Minor
    ["sirius", 6.7525, -16.7161, -1.46], ["mirzam", 6.3783, -17.9559, 1.98], ["adhara", 6.9771, -28.9721, 1.50],
    ["wezen", 7.1399, -26.3932, 1.83], ["aludra", 7.4016, -29.3031, 2.45], ["furud", 6.3385, -30.0634, 3.02],
    ["procyon", 7.6550, 5.2250, 0.34], ["gomeisa", 7.4525, 8.2894, 2.89],
    // Gemini
    ["castor", 7.5766, 31.8883, 1.58], ["pollux", 7.7553, 28.0262, 1.14], ["alhena", 6.6285, 16.3993, 1.93],
    ["mebsuta", 6.7322, 25.1311, 2.98], ["tejat", 6.3827, 22.5136, 2.87], ["wasat", 7.3354, 21.9823, 3.53],
    // Taurus
    ["aldebaran", 4.5987, 16.5092, 0.85], ["elnath", 5.4382, 28.6075, 1.65], ["alcyone", 3.7914, 24.1051, 2.87],
    ["tianguan", 5.6274, 21.1425, 3.00], ["theta2tau", 4.4777, 15.8709, 3.40], ["gammatau", 4.3299, 15.6275, 3.65],
    ["delta1tau", 4.3822, 17.5425, 3.76], ["epsilontau", 4.4769, 19.1805, 3.53],
    // Auriga
    ["capella", 5.2782, 45.9980, 0.08], ["menkalinan", 5.9921, 44.9475, 1.90], ["mahasim", 5.9954, 37.2125, 2.62],
    ["hassaleh", 4.9499, 33.1661, 2.69], ["almaaz", 5.0328, 43.8233, 2.98],
    // Perseus
    ["mirfak", 3.4054, 49.8612, 1.79], ["algol", 3.1361, 40.9556, 2.12], ["zetaper", 3.9022, 31.8836, 2.85],
    ["epsilonper", 3.9642, 40.0102, 2.89], ["gammaper", 3.0799, 53.5064, 2.91], ["deltaper", 3.7154, 47.7875, 3.01],
    // Cassiopeia
    ["caph", 0.1530, 59.1498, 2.27], ["schedar", 0.6751, 56.5373, 2.24], ["gammacas", 0.9451, 60.7167, 2.47],
    ["ruchbah", 1.4303, 60.2353, 2.68], ["segin", 1.9066, 63.6701, 3.37],
    // Andromeda, Pegasus
    ["alpheratz", 0.1398, 29.0906, 2.06], ["mirach", 1.1622, 35.6206, 2.05], ["almach", 2.0650, 42.3297, 2.10],
    ["deltaand", 0.6555, 30.8611, 3.27], ["markab", 23.0794, 15.2053, 2.48], ["scheat", 23.0629, 28.0828, 2.42],
    ["algenib", 0.2206, 15.1836, 2.83], ["enif", 21.7364, 9.8750, 2.39], ["homam", 22.6910, 10.8314, 3.40],
    // Ursa Major, Ursa Minor
    ["dubhe", 11.0621, 61.7511, 1.79], ["merak", 11.0307, 56.3824, 2.37], ["phecda", 11.8972, 53.6948, 2.44],
    ["megrez", 12.2571, 57.0326, 3.31], ["alioth", 12.9005, 55.9598, 1.77], ["mizar", 13.3988, 54.9254, 2.27],
    ["alkaid", 13.7923, 49.3133, 1.86], ["polaris", 2.5303, 89.2641, 1.98], ["kochab", 14.8451, 74.1555, 2.08],
    ["pherkad", 15.3455, 71.8340, 3.05], ["yildun", 17.5369, 86.5865, 4.35], ["epsilonumi", 16.7662, 82.0372, 4.21],
    ["zetaumi", 15.7343, 77.7945, 4.32], ["etaumi", 16.2917, 75.7553, 4.95],
    // Leo
    ["regulus", 10.1395, 11.9672, 1.35], ["denebola", 11.8177, 14.5720, 2.13], ["algieba", 10.3329, 19.8415, 2.08],
    ["zosma", 11.2351, 20.5237, 2.56], ["chertan", 11.2373, 15.4295, 3.33], ["etaleo", 10.1222, 16.7625, 3.49],
    ["adhafera", 10.2782, 23.4173, 3.44], ["rasalas", 9.8794, 26.0070, 3.88], ["epsilonleo", 9.7642, 23.7742, 2.98],
    // Bootes, Corona Borealis
    ["arcturus", 14.2610, 19.1824, -0.05], ["izar", 14.7498, 27.0742, 2.37], ["muphrid", 13.9114, 18.3977, 2.68],
    ["seginus", 14.5346, 38.3083, 3.03], ["nekkar", 15.0324, 40.3906, 3.49], ["deltaboo", 15.2584, 33.3148, 3.47],
    ["alphecca", 15.5781, 26.7147, 2.23],
    // Virgo, Corvus, Libra
    ["spica", 13.4199, -11.1614, 0.98], ["porrima", 12.6943, -1.4494, 2.74], ["vindemiatrix", 13.0363, 10.9592, 2.85],
    ["deltavir", 12.9267, 3.3975, 3.38], ["zetavir", 13.5782, -0.5958, 3.37], ["betavir", 11.8449, 1.7647, 3.61],
    ["gienahcrv", 12.2634, -17.5419, 2.59], ["kraz", 12.5731, -23.3967, 2.65], ["algorab", 12.4977, -16.5155, 2.95],
    ["epsiloncrv", 12.1687, -22.6198, 3.00], ["zubeneschamali", 15.2834, -9.3830, 2.61], ["zubenelgenubi", 14.8480, -16.0418, 2.75],
    // Scorpius
    ["antares", 16.4901, -26.4320, 1.06], ["acrab", 16.0906, -19.8055, 2.62], ["dschubba", 16.0056, -22.6217, 2.29],
    ["pisco", 15.9809, -26.1142, 2.89], ["sigmasco", 16.3531, -25.5928, 2.88], ["tausco", 16.5980, -28.2161, 2.82],
    ["epsilonsco", 16.8361, -34.2933, 2.29], ["mu1sco", 16.8645, -38.0475, 3.00], ["zeta2sco", 16.9097, -42.3614, 3.62],
    ["etasco", 17.2026, -43.2392, 3.32], ["sargas", 17.6220, -42.9978, 1.86], ["iota1sco", 17.7931, -40.1270, 2.99],
    ["kappasco", 17.7081, -39.0300, 2.39], ["shaula", 17.5601, -37.1038, 1.62], ["lesath", 17.5127, -37.2958, 2.70],
    // Sagittarius (the Teapot)
    ["kausaustralis", 18.4029, -34.3846, 1.85], ["nunki", 18.9211, -26.2967, 2.05], ["ascella", 19.0435, -29.8801, 2.60],
    ["kausmedia", 18.3499, -29.8281, 2.70], ["kausborealis", 18.4662, -25.4217, 2.81], ["alnasl", 18.0968, -30.4241, 2.98],
    ["phisgr", 18.7609, -26.9908, 3.17], ["tausgr", 19.1157, -27.6704, 3.32],
    // Crux, Centaurus, Triangulum Australe
    ["acrux", 12.4433, -63.0991, 0.76], ["mimosa", 12.7954, -59.6888, 1.25], ["gacrux", 12.5194, -57.1132, 1.64],
    ["deltacru", 12.2524, -58.7489, 2.79], ["rigilkentaurus", 14.6601, -60.8340, -0.27], ["hadar", 14.0637, -60.3730, 0.61],
    ["menkent", 14.1114, -36.3700, 2.06], ["epsiloncen", 13.6648, -53.4664, 2.30], ["gammacen", 12.6919, -48.9597, 2.17],
    ["etacen", 14.5918, -42.1578, 2.31], ["zetacen", 13.9257, -47.2883, 2.55], ["atria", 16.8111, -69.0277, 1.91],
    ["betatra", 15.9190, -63.4306, 2.83], ["gammatra", 15.3152, -68.6795, 2.87],
    // Carina, Vela, Puppis (the False Cross)
    ["canopus", 6.3992, -52.6958, -0.74], ["miaplacidus", 9.2200, -69.7172, 1.67], ["avior", 8.3752, -59.5097, 1.86],
    ["aspidiske", 9.2848, -59.2753, 2.21], ["deltavel", 8.7451, -54.7083, 1.96], ["kappavel", 9.3686, -55.0106, 2.47],
    ["suhail", 9.1333, -43.4326, 2.21], ["regor", 8.1589, -47.3366, 1.83], ["naos", 8.0597, -40.0031, 2.25],
    // Lyra, Cygnus, Aquila (the Summer Triangle)
    ["vega", 18.6156, 38.7837, 0.03], ["sheliak", 18.8347, 33.3627, 3.52], ["sulafat", 18.9824, 32.6895, 3.25],
    ["delta2lyr", 18.9084, 36.8986, 4.30], ["zeta1lyr", 18.7462, 37.6050, 4.36], ["deneb", 20.6905, 45.2803, 1.25],
    ["sadr", 20.3705, 40.2567, 2.23], ["gienahcyg", 20.7702, 33.9703, 2.48], ["deltacyg", 19.7496, 45.1308, 2.87],
    ["albireo", 19.5120, 27.9597, 3.08], ["etacyg", 19.9384, 35.0834, 3.89], ["altair", 19.8464, 8.8683, 0.77],
    ["tarazed", 19.7710, 10.6133, 2.72], ["alshain", 19.9219, 6.4068, 3.71], ["zetaaql", 19.0902, 13.8635, 2.99],
    ["deltaaql", 19.4250, 3.1147, 3.36], ["thetaaql", 20.1884, -0.8215, 3.26], ["lambdaaql", 19.1041, -4.8825, 3.43],
    // Bright stars outside the figures
    ["fomalhaut", 22.9608, -29.6222, 1.16], ["achernar", 1.6286, -57.2368, 0.46], ["alphard", 9.4598, -8.6586, 1.98],
    ["hamal", 2.1196, 23.4624, 2.00], ["diphda", 0.7265, -17.9866, 2.04], ["menkar", 3.0380, 4.0897, 2.54],
    ["rasalhague", 17.5822, 12.5600, 2.07], ["eltanin", 17.9434, 51.4889, 2.23], ["peacock", 20.4275, -56.7350, 1.94],
    ["alnair", 22.1372, -46.9611, 1.74], ["alderamin", 21.3096, 62.5856, 2.45], ["ankaa", 0.4381, -42.3061, 2.40]
]

// Stick figures: [name shown, [[from, to], ...]].
var constellations = [
    [Tr.t("Orión"), [["betelgeuse","bellatrix"],["betelgeuse","meissa"],["meissa","bellatrix"],["betelgeuse","alnitak"],
               ["bellatrix","mintaka"],["mintaka","alnilam"],["alnilam","alnitak"],["alnitak","saiph"],["mintaka","rigel"]]],
    [Tr.t("Can Mayor"), [["mirzam","sirius"],["sirius","wezen"],["wezen","adhara"],["wezen","aludra"],["adhara","furud"]]],
    [Tr.t("Can Menor"), [["procyon","gomeisa"]]],
    [Tr.t("Géminis"), [["castor","pollux"],["castor","mebsuta"],["mebsuta","tejat"],["pollux","wasat"],["wasat","alhena"]]],
    [Tr.t("Tauro"), [["gammatau","theta2tau"],["theta2tau","aldebaran"],["aldebaran","tianguan"],["gammatau","delta1tau"],
               ["delta1tau","epsilontau"],["epsilontau","elnath"]]],
    ["Auriga", [["capella","menkalinan"],["menkalinan","mahasim"],["mahasim","elnath"],["elnath","hassaleh"],
                ["hassaleh","almaaz"],["almaaz","capella"]]],
    [Tr.t("Perseo"), [["gammaper","mirfak"],["mirfak","deltaper"],["deltaper","epsilonper"],["epsilonper","zetaper"],["mirfak","algol"]]],
    [Tr.t("Casiopea"), [["caph","schedar"],["schedar","gammacas"],["gammacas","ruchbah"],["ruchbah","segin"]]],
    [Tr.t("Andrómeda"), [["alpheratz","deltaand"],["deltaand","mirach"],["mirach","almach"]]],
    [Tr.t("Pegaso"), [["markab","scheat"],["scheat","alpheratz"],["alpheratz","algenib"],["algenib","markab"],
                ["markab","homam"],["homam","enif"]]],
    [Tr.t("Osa Mayor"), [["dubhe","merak"],["merak","phecda"],["phecda","megrez"],["megrez","dubhe"],["megrez","alioth"],
                   ["alioth","mizar"],["mizar","alkaid"]]],
    [Tr.t("Osa Menor"), [["polaris","yildun"],["yildun","epsilonumi"],["epsilonumi","zetaumi"],["zetaumi","kochab"],
                   ["kochab","pherkad"],["pherkad","etaumi"],["etaumi","zetaumi"]]],
    ["Leo", [["regulus","etaleo"],["etaleo","algieba"],["algieba","adhafera"],["adhafera","rasalas"],["rasalas","epsilonleo"],
             ["algieba","zosma"],["zosma","denebola"],["denebola","chertan"],["chertan","regulus"]]],
    [Tr.t("Boyero"), [["arcturus","izar"],["izar","deltaboo"],["deltaboo","nekkar"],["nekkar","seginus"],["seginus","arcturus"],
                ["arcturus","muphrid"]]],
    ["Virgo", [["betavir","porrima"],["porrima","deltavir"],["deltavir","vindemiatrix"],["porrima","spica"],
               ["spica","zetavir"],["zetavir","deltavir"]]],
    [Tr.t("Cuervo"), [["gienahcrv","algorab"],["algorab","kraz"],["kraz","epsiloncrv"],["epsiloncrv","gienahcrv"]]],
    ["Libra", [["zubeneschamali","zubenelgenubi"]]],
    [Tr.t("Escorpio"), [["acrab","dschubba"],["dschubba","pisco"],["dschubba","sigmasco"],["sigmasco","antares"],
                  ["antares","tausco"],["tausco","epsilonsco"],["epsilonsco","mu1sco"],["mu1sco","zeta2sco"],
                  ["zeta2sco","etasco"],["etasco","sargas"],["sargas","iota1sco"],["iota1sco","kappasco"],
                  ["kappasco","shaula"],["shaula","lesath"]]],
    [Tr.t("Sagitario"), [["alnasl","kausmedia"],["kausmedia","kausaustralis"],["kausaustralis","alnasl"],
                   ["kausmedia","kausborealis"],["kausborealis","phisgr"],["phisgr","kausmedia"],["phisgr","nunki"],
                   ["nunki","tausgr"],["tausgr","ascella"],["ascella","phisgr"],["ascella","kausaustralis"]]],
    [Tr.t("Cruz del Sur"), [["acrux","gacrux"],["mimosa","deltacru"]]],
    [Tr.t("Centauro"), [["rigilkentaurus","hadar"],["hadar","epsiloncen"],["epsiloncen","gammacen"],["epsiloncen","zetacen"],
                  ["zetacen","etacen"],["etacen","menkent"]]],
    [Tr.t("Triángulo Austral"), [["atria","betatra"],["betatra","gammatra"],["gammatra","atria"]]],
    [Tr.t("Falsa Cruz"), [["avior","kappavel"],["deltavel","aspidiske"]]],
    [Tr.t("Lira"), [["vega","zeta1lyr"],["zeta1lyr","sheliak"],["sheliak","sulafat"],["sulafat","delta2lyr"],["delta2lyr","zeta1lyr"]]],
    [Tr.t("Cisne"), [["deneb","sadr"],["sadr","etacyg"],["etacyg","albireo"],["deltacyg","sadr"],["sadr","gienahcyg"]]],
    [Tr.t("Águila"), [["tarazed","altair"],["altair","alshain"],["alshain","thetaaql"],["altair","deltaaql"],
                ["deltaaql","lambdaaql"],["deltaaql","zetaaql"]]]
]

// Greenwich mean sidereal time, in degrees, at a UTC instant in milliseconds (IAU 1982,
// first order: good to a fraction of a second of time over this century).
function siderealDegrees(ms) {
    var days = ms / 86400000 + 2440587.5 - 2451545.0
    var g = (280.46061837 + 360.98564736629 * days) % 360
    return g < 0 ? g + 360 : g
}

// A sky direction in the globe's own Earth-fixed frame (Greenwich +Z, 90 degrees east +X,
// north +Y): the star's hour angle at Greenwich is its right ascension minus sidereal time.
function direction(raHours, decDeg, sidereal) {
    var lambda = (raHours * 15 - sidereal) * Math.PI / 180
    var phi = decDeg * Math.PI / 180
    return {x: Math.cos(phi) * Math.sin(lambda), y: Math.sin(phi), z: Math.cos(phi) * Math.cos(lambda)}
}

// The faint background: stars below the figures' magnitudes are too many to list, so
// they are generated, with a fixed seed so the field never changes, and denser toward the
// Milky Way's real plane (north galactic pole at RA 12h51.4m, Dec +27.13, J2000), where the
// sky really is crowded. The named stars and figures above are catalogue; this field is not.
function faintField(count) {
    var seed = 20261028
    function random() {
        seed = (seed + 0x6D2B79F5) | 0
        var t = Math.imul(seed ^ (seed >>> 15), 1 | seed)
        t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
        return ((t ^ (t >>> 14)) >>> 0) / 4294967296
    }
    var pole = direction(12.857, 27.128, 0), out = []
    while (out.length < count) {
        var z = 2 * random() - 1, a = 2 * Math.PI * random(), r = Math.sqrt(1 - z * z)
        var v = {x: r * Math.cos(a), y: z, z: r * Math.sin(a)}
        var b = Math.asin(Math.max(-1, Math.min(1, v.x * pole.x + v.y * pole.y + v.z * pole.z))) * 180 / Math.PI
        if (random() * 4 > 1 + 3 * Math.exp(-(b * b) / 225)) continue
        // Back to right ascension and declination, so the field turns with the rest.
        var dec = Math.asin(v.y) * 180 / Math.PI
        var ra = (Math.atan2(v.x, v.z) * 180 / Math.PI + 360) % 360 / 15
        out.push([ra, dec, 3.8 + 2.7 * Math.pow(random(), 0.6)])
    }
    return out
}
var field = faintField(2600)

// Every star and figure placed for one instant.
function sky(ms) {
    var g = siderealDegrees(ms), byId = {}, placed = [], faint = []
    for (var f = 0; f < field.length; ++f) {
        var fd = direction(field[f][0], field[f][1], g)
        fd.mag = field[f][2]
        faint.push(fd)
    }
    for (var i = 0; i < stars.length; ++i) {
        var s = stars[i], d = direction(s[1], s[2], g)
        d.mag = s[3]; d.id = s[0]
        byId[s[0]] = d; placed.push(d)
    }
    var figures = constellations.map(function(c) {
        var segments = [], members = {}
        c[1].forEach(function(pair) {
            var a = byId[pair[0]], b = byId[pair[1]]
            if (a && b) { segments.push([a, b]); members[pair[0]] = a; members[pair[1]] = b }
        })
        var cx = 0, cy = 0, cz = 0, n = 0
        for (var k in members) { cx += members[k].x; cy += members[k].y; cz += members[k].z; ++n }
        var len = Math.sqrt(cx * cx + cy * cy + cz * cz) || 1
        return {name: c[0], segments: segments, centre: {x: cx / len, y: cy / len, z: cz / len}}
    })
    return {stars: placed, figures: figures, faint: faint}
}

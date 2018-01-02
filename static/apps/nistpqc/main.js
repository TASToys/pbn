problems = [
	"?",			//0
	"Code",			//1
	"PoSSo",		//2
	"Lattice",		//3
	"Lattice(NTRU)",	//4
	"Multivariate",		//5
	"Hash",			//6
	"Hypercomplex",		//7
	"Code(Rank)",		//8
	"Isogeny",		//9
];

proposals = {
	"AKCN":{"id":0, "problem":0, "signature":false},
	"BIG QUAKE":{"id":1, "problem":1, "signature":false},
	"BIKE":{"id":2, "problem":1, "signature":false},
	"CFPKM":{"id":3, "problem":2, "signature":false},
	"Classic McEliece":{"id":4, "problem":1, "signature":false},
	"CNKE":{"id":5, "problem":0, "signature":false},
	"Compact-LWE":{"id":6, "problem":3, "signature":false},
	"CRYSTALS-Dilithium":{"id":7, "problem":3, "signature":true},
	"CRYSTALS-Kyber":{"id":8, "problem":3, "signature":false},
	"DAGS":{"id":9, "problem":1, "signature":false},
	"Ding Key Exchange":{"id":10, "problem":3, "signature":false},
	"DME-KEM":{"id":11, "problem":0, "signature":false},
	"DRS":{"id":12, "problem":0, "signature":true},
	"DualModeMS":{"id":13, "problem":0, "signature":true},
	"EDON-K":{"id":14, "problem":0, "signature":false},
	"EMBLEM":{"id":15, "problem":3, "signature":false},
	"R.EMBLEM":{"id":16, "problem":3, "signature":false},
	"Falcon":{"id":17, "problem":4, "signature":true},
	"FrodoKEM":{"id":18, "problem":3, "signature":false},
	"GeMSS":{"id":19, "problem":5, "signature":true},
	"Giophantus":{"id":20, "problem":0, "signature":false},
	"Gravity-SPHINCS":{"id":21, "problem":6, "signature":true},
	"GUESS AGAIN":{"id":22, "problem":0, "signature":false},
	"Gui":{"id":23, "problem":5, "signature":true},
	"HILA5":{"id":24, "problem":3, "signature":false},
	"HiMQ-3":{"id":25, "problem":5, "signature":true},
	"HK17":{"id":26, "problem":7, "signature":false},
	"HQC":{"id":27, "problem":1, "signature":false},
	"KINDI":{"id":28, "problem":0, "signature":false},
	"LAC":{"id":29, "problem":0, "signature":false},
	"LAKE":{"id":30, "problem":8, "signature":false},
	"LEDAkem":{"id":31, "problem":0, "signature":false},
	"LEDApkc":{"id":32, "problem":0, "signature":false},
	"Lepton":{"id":33, "problem":0, "signature":false},
	"LIMA":{"id":34, "problem":0, "signature":false},
	"Lizard":{"id":35, "problem":0, "signature":false},
	"LOCKER":{"id":36, "problem":8, "signature":false},
	"LOTUS":{"id":37, "problem":0, "signature":false},
	"LUOV":{"id":38, "problem":5, "signature":true},
	"McNie":{"id":39, "problem":8, "signature":false},
	"Mersenne-756839":{"id":40, "problem":0, "signature":false},
	"MQDSS":{"id":41, "problem":5, "signature":true},
	"NewHope":{"id":42, "problem":3, "signature":false},
	"NTRUEncrypt":{"id":43, "problem":4, "signature":false},
	"NTRU-HRSS-KEM":{"id":44, "problem":4, "signature":false},
	"NTRU Prime":{"id":45, "problem":4, "signature":false},
	"NTS-KEM":{"id":46, "problem":0, "signature":false},
	"Odd Manhattan":{"id":47, "problem":3, "signature":false},
	"OKCN":{"id":48, "problem":0, "signature":false},
	"Ouroboros-R":{"id":49, "problem":8, "signature":false},
	"Picnic":{"id":50, "problem":0, "signature":true},
	"pqNTRUSign":{"id":51, "problem":4, "signature":true},
	"pqsigRM":{"id":52, "problem":0, "signature":true},
	"QC-MDPC KEM":{"id":53, "problem":0, "signature":false},
	"qTESLA":{"id":54, "problem":3, "signature":true},
	"RaCoSS":{"id":55, "problem":0, "signature":true},
	"Rainbow":{"id":56, "problem":5, "signature":true},
	"Ramstake":{"id":57, "problem":0, "signature":false},
	"RankSign":{"id":58, "problem":8, "signature":true},
	"RLCE-KEM":{"id":59, "problem":0, "signature":false},
	"Round2":{"id":60, "problem":3, "signature":false},
	"RQC":{"id":61, "problem":8, "signature":false},
	"RVB":{"id":62, "problem":0, "signature":false},
	"SABER":{"id":63, "problem":0, "signature":false},
	"SIKE":{"id":64, "problem":9, "signature":false},
	"SPHINCS+":{"id":65, "problem":6, "signature":true},
	"SRTPI":{"id":66, "problem":0, "signature":false},
	"ThreeBears":{"id":67, "problem":3, "signature":false},
	"Titanium":{"id":68, "problem":0, "signature":false},
	"TPSig":{"id":69, "problem":0, "signature":true},
	"WalnutDSA":{"id":70, "problem":0, "signature":true},
};

function h1_text(text)
{
	var x = document.createElement("h1");
	x.appendChild(document.createTextNode(text));
	return x;
}

function th_text(text)
{
	var x = document.createElement("th");
	x.appendChild(document.createTextNode(text));
	return x;
}

function td_text(text)
{
	var x = document.createElement("td");
	x.appendChild(document.createTextNode(text));
	return x;
}

function main()
{
	document.body.appendChild(h1_text("Encryption/Key Exchange:"));
	var enc_table = document.createElement("table");
	var enc_table_headers = document.createElement("tr");
	enc_table_headers.appendChild(th_text("Name"));
	enc_table_headers.appendChild(th_text("Problem"));
	enc_table.appendChild(enc_table_headers);
	for (var prop in proposals) { if (proposals.hasOwnProperty(prop) && !proposals[prop].signature) {
		var row = document.createElement("tr");
		row.appendChild(td_text(prop));
		row.appendChild(td_text(problems[proposals[prop].problem]));
		enc_table.appendChild(row);
	}}
	document.body.appendChild(enc_table);

	document.body.appendChild(h1_text("Signatures:"));
	var sig_table = document.createElement("table");
	var sig_table_headers = document.createElement("tr");
	sig_table_headers.appendChild(th_text("Name"));
	sig_table_headers.appendChild(th_text("Problem"));
	sig_table.appendChild(sig_table_headers);
	for (var prop in proposals) { if (proposals.hasOwnProperty(prop) && proposals[prop].signature) {
		var row = document.createElement("tr");
		row.appendChild(td_text(prop));
		row.appendChild(td_text(problems[proposals[prop].problem]));
		sig_table.appendChild(row);
	}}
	document.body.appendChild(sig_table);
}

window.addEventListener('load', main, false);

# Eine Million kurze Texte zusammenbauen und vergleichen. Misst Texte und den Speicher dafür.

kleiner = 0
gleich = 0
i = 1
while i <= 1000000:
    s = "nr" + str(i) + "!"
    if s < "nr5":
        kleiner += 1
    if s == "nr777!":
        gleich += 1
    i += 1
print(kleiner, gleich)

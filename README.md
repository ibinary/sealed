# Sealed
Simple Media Ownership, Copyright, and License Protection Utility MIT License: Jake Kitchen - https://github.com/qqa112811 and Ken Nickerson - kenn@ibinary.com

Sealed is an open-source utility that employs a novel process to protect original creator or copyright holder media. Initially built for IMAGE(S), Sealed v2.0 will follow a similar process to include AUDIO(S), VIDEO(S) and TEXT(S). The goal is to help protect original content creators and copyright holders from the growing incursions on their art, that could be proven manually, automatically, or legally in any process the copyright holder may engage in.

Sealed employs a novel process to verify and document ownership, while providing an immediately shareable asset. This process provides a proof-based model of ownership that is resistant to AI scrapping, GPT refactoring, decryption, or other processes that may use or alter source images (e.g., outpainting) and deliberate theft of copyright. The code for sealed is open-source, under the MIT License, and may be included in as many applets, applications, and services as possible with attribution and a link to this repository.
# Process:
Sealed invokes a process where media can be measured, cropped, shared, much like "edges" on paintings used for anti-forgery and insurance process. Standard hash-codes are generated to text and .json files to be secured personally, or on a service like IPFS or blockchain or redis or any preferred secured store.

1. Copyright IMAGE(S) are uploaded to Sealed.ch OR local terminal application OR self-directed use of the open-source code - https://github.com/ibinary/sealed integrated for custom solutions. VIDEO(S) and TEXT(S) will be part of Sealed 2.0.
2. IMAGE(S) is HASHED to document or fingerprint the original.
3. IMAGE(S) is randomly cropped from 3 to 11 pixels depending on IMAGE(S) size, producing a separate file of frames or "edges."
4. Post crop IMAGE(S) (3) are HASHED.
5. Post crop EDGE(S) (3) are HASHED.
6. .ZIP file is produced with: original IMAGE(S), cropped IMAGE(S), edges IMAGE(S) and HASH in .TXT and .JSON formats.
7. Post crop original "share" IMAGE(S) are available for immediate distribution.
8. Option to include QR Code (7) to reference contact, URL, and other information for sharing.

![sealed-process](https://github.com/ibinary/sealed/assets/86942/c543eaf1-df50-4908-8843-8ce0fc2679e8)


# Post Process:
1. If a post-process, shared copyright IMAGE(S) is repurposed, the original copyright owner has a documented file (1…8) to confirm ownership of original copyright material in absolute terms.
2. Post-process .ZIP contains .txt and .json files that can be stored locally, or imported into a database archive or monitoring tool.
# Post Release 1.0:
1. Expansion of process is possible with secure store or public share (e.g., IPFS) of post process media.
2. Expansion of secure store to a distributed blockchain like store for immediate image compare, registration, certification.
3. Expansion of media types to AUDIO(S), VIDEO(S) and TEXT(S) in Sealed v2.0.
4. Legal precedent to verify the efficacy of Sealed by a copyright holder.
# History:
The idea for sealed was prompted by a chance conversation at Musée d'Orsay - https://www.musee-orsay.fr/en in 2010. I asked about the insurance process for paintings in the gallery, and learned about scanning or photographing "edges" as a prime defense against forgery. As the content industry has changed with the move from analog to digital (no (print) negatives) and more recently scrapped for use in corpus for AI used in GPT, a need has grown to have a simple, secure, open-source method to secure copyright.
"The Son of Man" (French: Le fils de l'homme) - https://en.wikipedia.org/wiki/The_Son_of_Man - is a 1964 painting by the Belgian surrealist painter René Magritte was chosen for Sealed.ch homepage, as a reflection of the use of this process in the popular 1999 movie "The Thomas Crown Affair" - https://en.wikipedia.org/wiki/The_Thomas_Crown_Affair_(1999_film).
# Contact:
We hope others can leverage this process, code into their products, services and applications to ensure protection for creators (copyright holders), who are appreciated for their work, but often not respected in terms of attribution or compensation. If you have any suggestions, enhancements, updates, forks, all are warmly welcomed at sealed-ch@pm.me
#
Sealed was privately funded by iBinary LLC. Follow Sealed on Twitter: https://twitter.com/sealedch

# sealed
Simple Media Ownership, Copyright, and License Protection Utility
MIT License - Developed by Jake Kitchen - Jake Kitchen - https://github.com/qqa112811 and Ken Nickerson - kenn@ibinary.com - https://github.com/kcnickerson - @kcnickerson - Sealed was privately funded by iBinary LLC.
#
The purpose of "sealed" is to offer a simple utility to protect original creator or copyright holder media. Sealed employs a novel use of a known method to verify and document ownership, while providing a shareable asset. This process provides a proof-based model of ownership that is resistant to AI scrapping, GPT refactoring, decryption or other processes that may use or alter source images (e.g. outpainting) and deliberate theft of copyright.
#
The goal is to help protect original content creators for the growing incursions on their art (media) that could be proven in any process the copyright holder may engage.
#
The idea for sealed was initiated after a chance conversation at Musée d'Orsay - https://www.musee-orsay.fr/en in 2010. I asked about the insurance process for paintings in the gallery, and learned about scanning or photographing "edges" as a prime defense against forgery. As the content industry has changed with the move from analog to digital (no (print) negatives) and more recently scrapped for use in corpus for AI, a need has grown to have a simple, secure, open-source method to secure copyright. "The Son of Man" (French: Le fils de l'homme) - https://en.wikipedia.org/wiki/The_Son_of_Man - is a 1964 painting by the Belgian surrealist painter René Magritte was chosen for Sealed.ch homepage, as a reflection of the use of this process in the popular 1999 movie "The Thomas Crown Affair" - https://en.wikipedia.org/wiki/The_Thomas_Crown_Affair_(1999_film).
#
The core idea is to invoke a process where media can be measured, cropped, shared, much like "edges" on paintings used for insurance process. Standard CRCs (hash) in the process can be secured personally, or on a public share like IPFS - https://www.ipfs.com - blockchain or redis like, secured store.
#
Process:
1. Copyright Image uploaded to Sealed.ch OR local terminal application OR self-directed use of the open-source library - https://github.com/ibinary/sealed - integrated into custom solutions.
2. Image is CRC to document the original image hash(s).
3. Image is randomly cropped from 3 to 11 pixels, producing a separate file of frames or edges.
4. Post crop image (3) is CRC.
5. Post crop edge (3) is CRC.
6. .ZIP file produced with: original image, cropped image, edges and CRC .TXT / .JSON.
7. Post crop original image is available for immediate distribution.
8. Option to include QR Code (7) to reference contact, URL, and other information for sharing.
#
Post Process:
a. If a post-process, shared copyright image is repurposed, the original copyright owner has a documented file (1…8) to confirm ownership of original copyright material on absolute terms.
b. Post-process .zip contains .txt and .json files that can be stored locally, or imported into a database archive.
#
Post Release 1.0
c. Expansion of process is possible with secure store or public share (e.g., IPFS) of post process media.
d. Expansion of secure store to a distributed blockchain like store for immediate image compare, registration, certification.
e. Expansion of media types to audio and video in version 2.0.
…
z. Legal precedent to verify the efficacy of Sealed by a copyright holder.
#
We hope others can leverage this process, code into their products, services and applications to ensure protection for the creative set, who are appreciated for their work, but often not respected in terms of attribution or compensation. If you have any suggestions, enhancements, updates, forks, all are warmly welcomed at contact@sealed.ch. Good luck!

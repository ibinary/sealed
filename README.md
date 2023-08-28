# sealed
Simple Media Ownership and Copyright Protection Utility
MIT License - Funded by iBinary LLC, Developed by Jake Kitchen and Ken Nickerson
#
The purpose of "sealed" is to offer open-source code to support the verification and registration of media, to offer a novel method to document ownership, provide shareable media and register CRC for permanenet file. This process provides a proof model for media ownership that is resistant to AI, decryption or other processes to alter source images (e.g. outpainting) or deliberate theft of copyright.
#
The goal is to help protect original content creators for the growing incursions on their "art" (media) that could be proven in any process the copyright holder may engage.
#
The idea for sealed was the secondary effect of a chance conversation at Musée d'Orsay - https://www.musee-orsay.fr/en in early 2000s when asking about the insurance process for paintings in the gallery. As the content industry has changed with the move from analog to digital (no (print) negatives) and more recently use in corpus for AI, a need has grown to have a simple, secure, open0source method to secure copyright.
#
iBinary LLC funded this development, idea/architecutre by Ken Nickerson, execution (code) by Jake Kitchen.

Jake Kitchen - https://github.com/qqa112811 - jake@sealed.ch - @?
Ken Nickerson - https://github.com/kcnickerson - kenn@ibinary.com - @kcnickerson
#
The core idea is to invoke a process where media can be measured, cropped, shared like "edges" on paintings and standard CRCs on the process that can be secured personaly, or on a public share like IPFS - https://www.ipfs.com or blockchain or redis based, secured stores.
#
Process:
1. Copyright Image uploaded to Sealed.ch OR direct use from the open-source library - https://github.com/ibinary/sealed integrated into custom solutions.
2. Image is CRC to document original image hash.
3. Image is randomly cropped from 3-11 pixels.
4. Post crop image is CRC.
5. Post crop edge (3) is CRC.
6. .ZIP file produced with: original image, cropped image, edges and CRC .TXT / .JSON.
7. Post crop original image is available for distribution.
8. Possible option to inculde a QR Code (7) to reference contact, URL and other information as an option for sharing.
#
Post Process:
a. If a post-process, shared copyright image is repurposed, the original copyright owner has a documented file (1...8) to confirm ownership of original copyright material.
Post Release 1.0
b. Expansion of process is possible with secure store or public share (e.g. IPFS) of post process media.
c. Expansion of secure store to a distributed blockchain like store for immediate image compare, registration, certification.
#
Longer Term:
z. Legal precident to verify the efficasy of the process by a copyright holder.
#
We hope others can leverage this process, code into their products, services and applications to ensure protection for the creative set, who are appreciated for their work, but often not respected in terms of attribution or compensation.
#
If you have any suggestions, enhancements, updates, forks, all are warmly welcomed. Good luck!

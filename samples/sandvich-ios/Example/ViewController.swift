/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import UIKit

class ViewController: UIViewController {

    override func viewDidLoad() {
        super.viewDidLoad()
        self.view.backgroundColor = .white

        let button = UIButton(frame: CGRect(x: 100, y: 100, width: 100, height: 50))
        button.center = self.view.center
        button.backgroundColor = .blue
        button.setTitle("Log-in", for: [])
        button.addTarget(self, action: #selector(onButtonPressed), for: .touchUpInside)

        self.view.addSubview(button)
    }

    @objc func onButtonPressed(sender: UIButton!) {
        self.navigationController?.pushViewController(FxAView(), animated: true)
    }

    override func didReceiveMemoryWarning() {
        super.didReceiveMemoryWarning()
        // Dispose of any resources that can be recreated.
    }


}


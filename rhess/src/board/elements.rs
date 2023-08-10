use std::cell::UnsafeCell;
use super::Board;

pub trait Piece
{
     fn move_piece(&mut self,new_pos: (usize,usize)) -> Result<(),IllegalMoveErr>;
     fn check_moves(&self) -> Vec::<(usize,usize)>;
     fn getch(&self) ->&str;
}




pub enum PlayerColor{WHITE,BLACK}


pub static WHT_PCS_UTF8 : &'static str ="♟︎♜♞♝♛♚";
pub static BLK_PCS_UTF8 : &'static str ="♙♖♘♗♕♔";

pub static BLK_SQUARE : &'static str = "⬛";
pub static WHT_SQUARE : &'static str = "⬜";

struct IllegalMoveErr{ err_msg : String}


pub struct Pawn
{
    pub position : (usize,usize),
    pub color : PlayerColor,
    board : UnsafeCell<Board>
}

impl Pawn
{
    pub fn new(color : PlayerColor , column : u8, board:UnsafeCell<Board>) -> Pawn
    {
        let row : usize;
        let rep : &'static str;
        match color 
        { 
            PlayerColor::WHITE => {
            row = 1;
            },
            PlayerColor::BLACK => {
            row = 6;
            }
        }
        Pawn{
            color:color,
            position : (row ,column as usize),
            board:board }
    }

}

impl Piece for Pawn
{


    fn check_moves(&self) -> Vec::<(usize,usize)>
    {
        let mut valids = Vec::<(usize,usize)>::new();
        match self.color
        {
            PlayerColor::WHITE =>
        {
            if self.position.0 != 7
            {
                valids.push((self.position.0,self.position.1+1));
                if self.position.1!=0
                {
                    match (self.board.get_mut()).arr[self.position.0 +1 as usize][self.position.1-1 as usize]
                    {
                        None =>(),
                        Some(x) =>  if matches!(x.color,PlayerColor::BLACK) {valids.push((self.position.0+1,self.position.1-1));}
                    }
                }
                if self.position.1!=7
                {
                    match (self.board.get_mut()).arr[self.position.0+1][self.position.1+1]
                    {
                        None =>(),
                        Some(x) => if matches!(x.color,PlayerColor::BLACK) {valids.push((self.position.0+1,self.position.1+1));}
                    }
                }
            }
            if self.position.0 == 1 {valids.push((self.position.0,self.position.1+2));}
        }

        PlayerColor::BLACK => 
        {
            if self.position.0 != 0
            {
                valids.push((self.position.0,self.position.1-1));
                if self.position.1!=0
                {
                    match (self.board.get_mut()).arr[self.position.0-1][self.position.1-1]
                    {
                        None =>(),
                        Some(x) =>  if matches!(x.color,PlayerColor::WHITE) {valids.push((self.position.0-1,self.position.1-1));}
                    }
                }
                if self.position.1!=7
                {
                    match (self.board.get_mut()).arr[self.position.0+1][self.position.1+1]
                    {
                        None =>(),
                        Some(x) => if  matches!(x.color,PlayerColor::WHITE) {valids.push((self.position.0-1,self.position.1+1));}
                    }
                }
            }
            if self.position.0 == 6 {valids.push((self.position.0,self.position.1-2));}
        }
    }

        return valids;
    
}
  

    fn move_piece(&mut self, new_pos : (usize,usize)) -> Result<(),IllegalMoveErr>
    {
        for possible_pos in self.check_moves()
        {
            if new_pos == possible_pos
            {
                (self.board.get_mut()).arr[self.position.0][self.position.1] = None;
                (self.board.get_mut()).arr[new_pos.0][new_pos.1] = Some(UnsafeCell::new(self));
                return Ok(());
            }
        }
        return Err(IllegalMoveErr { err_msg: "".to_string() });
    }

    fn getch(&self) -> &str
    {
        match self.color
        {
            PlayerColor::BLACK=> return "♙",
            PlayerColor::WHITE => return "♟︎",
        }
    }
}

